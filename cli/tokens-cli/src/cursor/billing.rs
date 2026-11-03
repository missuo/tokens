use super::{
    CURSOR_HTTP_TIMEOUT, CURSOR_MAX_JSON_BYTES, CURSOR_MAX_JSON_PAGES, cursor_http_client_builder,
    read_cursor_body_with_cap,
};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::{Duration as Timeout, Instant};

const ENDPOINT: &str = "https://cursor.com/api/dashboard/v2/query-billing-items";
const EVENT_PREFIX: &str = "billing:";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    data: Vec<BillingItem>,
    pagination: Pagination,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pagination {
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BillingItem {
    billing_item_id: String,
    event_time: DateTime<Utc>,
    product: String,
    charge_category: String,
    usage: Option<Value>,
    display_extras: Option<DisplayExtras>,
}

#[derive(Deserialize)]
struct DisplayExtras {
    model: Option<String>,
}

impl BillingItem {
    fn into_event(self) -> Result<Option<Value>> {
        if self.product != "cursor" || self.charge_category != "usage" {
            return Ok(None);
        }
        let Some(usage) = self.usage else {
            return Ok(None);
        };
        let count = |key: &str| -> Result<i64> {
            let Some(value) = usage.get(key) else {
                return Ok(0);
            };
            let n = value
                .as_i64()
                .or_else(|| value.as_str()?.parse().ok())
                .context("Invalid Cursor billing token count")?;
            ensure!(n >= 0, "Negative Cursor billing token count");
            Ok(n)
        };
        let input = count("prompt-text-tokens")?;
        let output = count("completion-text-tokens")?;
        let read = count("cached-prompt-text-tokens")?;
        let write = count("cache-write-text-tokens")?;
        if [input, output, read, write] == [0; 4] {
            return Ok(None);
        }
        let model = self
            .display_extras
            .and_then(|extras| extras.model)
            .filter(|model| !model.trim().is_empty())
            .context("Cursor billing item has no model")?;
        ensure!(
            !self.billing_item_id.trim().is_empty(),
            "Cursor billing item has no ID"
        );
        Ok(Some(json!({
            "usageEventId": format!("{EVENT_PREFIX}{}", self.billing_item_id),
            "timestamp": self.event_time.timestamp_millis(),
            "model": model,
            "tokenUsage": {
                "inputTokens": input,
                "outputTokens": output,
                "cacheReadTokens": read,
                "cacheWriteTokens": write,
            },
        })))
    }
}

pub(super) fn is_billing_event(event: &Value) -> bool {
    event
        .get("usageEventId")
        .and_then(Value::as_str)
        .is_some_and(|id| id.starts_with(EVENT_PREFIX))
}

// The two APIs have different event IDs. Prefer a billing record over a
// legacy record only when their timestamp, model, and all four counts agree.
pub(super) fn reconcile_legacy(
    mut cached: Vec<Value>,
    mut incoming: Vec<Value>,
) -> (Vec<Value>, Vec<Value>) {
    let billing_rows: std::collections::HashSet<_> = cached
        .iter()
        .chain(&incoming)
        .filter(|event| is_billing_event(event))
        .filter_map(comparable_usage)
        .collect();
    if !billing_rows.is_empty() {
        let keep = |event: &Value| {
            is_billing_event(event)
                || comparable_usage(event).is_none_or(|usage| !billing_rows.contains(&usage))
        };
        cached.retain(keep);
        incoming.retain(keep);
    }
    (cached, incoming)
}

fn comparable_usage(event: &Value) -> Option<(i64, String, [i64; 4])> {
    let number = |value: &Value| value.as_i64().or_else(|| value.as_str()?.parse().ok());
    let tokens = event.get("tokenUsage")?;
    let count = |key| tokens.get(key).map_or(Some(0), number);
    Some((
        number(event.get("timestamp")?)?,
        event.get("model")?.as_str()?.to_owned(),
        [
            count("inputTokens")?,
            count("outputTokens")?,
            count("cacheReadTokens")?,
            count("cacheWriteTokens")?,
        ],
    ))
}

pub(super) async fn fetch(
    token: &str,
    timeout: Option<Timeout>,
    since: Option<i64>,
) -> Result<String> {
    fetch_from(ENDPOINT, token, timeout, since).await
}

async fn fetch_from(
    endpoint: &str,
    token: &str,
    timeout: Option<Timeout>,
    since: Option<i64>,
) -> Result<String> {
    // Use the HTTP/1.1 API route. The browser-style request through the
    // default client returned a challenge instead of JSON in live testing.
    let client = cursor_http_client_builder()
        .http1_only()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut cookie =
        reqwest::header::HeaderValue::from_str(&format!("WorkosCursorSessionToken={token}"))
            .context("Invalid Cursor session cookie")?;
    cookie.set_sensitive(true);
    let now = Utc::now();
    // The API accepts windows of at most 31 days. Bootstrap from its most
    // recent window, or walk consecutive windows when a cache is older.
    let mut start = since
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(|| now - Duration::days(31));
    let deadline = Instant::now() + timeout.unwrap_or(CURSOR_HTTP_TIMEOUT);
    let mut remaining_bytes = CURSOR_MAX_JSON_BYTES;
    let mut events = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..CURSOR_MAX_JSON_PAGES {
        let end = (start + Duration::days(31)).min(now);
        let remaining = deadline.saturating_duration_since(Instant::now());
        ensure!(!remaining.is_zero(), "Cursor billing sync timed out");
        let mut request = client
            .get(endpoint)
            .header(reqwest::header::COOKIE, cookie.clone())
            .header("Accept", "application/json")
            .query(&[
                ("scope", "user".to_owned()),
                ("product", "cursor".to_owned()),
                ("pageSize", "500".to_owned()),
                (
                    "startTime",
                    start.to_rfc3339_opts(SecondsFormat::Millis, true),
                ),
                ("endTime", end.to_rfc3339_opts(SecondsFormat::Millis, true)),
            ])
            .timeout(remaining);
        if let Some(cursor) = &cursor {
            request = request.query(&[("cursor", cursor)]);
        }
        let response = request.send().await?;
        ensure!(
            response.status().is_success(),
            "Cursor billing API returned {}",
            response.status()
        );
        let text =
            read_cursor_body_with_cap(response, remaining_bytes, "billing items JSON").await?;
        remaining_bytes -= text.len();
        let page: Page = serde_json::from_str(&text).context("Invalid Cursor billing response")?;
        for item in page.data {
            if let Some(event) = item.into_event()? {
                events.push(event);
            }
        }
        if let Some(next) = page.pagination.next_cursor.filter(|next| !next.is_empty()) {
            ensure!(
                cursor.as_ref() != Some(&next),
                "Cursor billing pagination did not advance"
            );
            cursor = Some(next);
        } else if end == now {
            return Ok(json!({"usageEventsDisplay": events}).to_string());
        } else {
            start = end;
            cursor = None;
        }
    }
    anyhow::bail!("Cursor billing pagination exceeded the request limit")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_billing_rows_and_preserves_nonoverlapping_legacy_usage() {
        let raw = json!({
            "billingItemId": "fixture",
            "eventTime": "2026-11-01T06:05:09.180Z",
            "product": "cursor",
            "chargeCategory": "usage",
            "usage": {
                "prompt-text-tokens": "382",
                "completion-text-tokens": "453",
                "cached-prompt-text-tokens": "7",
                "cache-write-text-tokens": "3",
            },
            "displayExtras": {"model": "model"},
        });
        let event = serde_json::from_value::<BillingItem>(raw.clone())
            .unwrap()
            .into_event()
            .unwrap()
            .unwrap();
        assert_eq!(
            event["tokenUsage"],
            json!({
                "inputTokens": 382, "outputTokens": 453, "cacheReadTokens": 7, "cacheWriteTokens": 3,
            })
        );
        let mut bot = raw;
        bot["product"] = json!("grok-bot");
        assert!(
            serde_json::from_value::<BillingItem>(bot)
                .unwrap()
                .into_event()
                .unwrap()
                .is_none()
        );

        let mut duplicate = event.clone();
        duplicate["usageEventId"] = json!("legacy");
        duplicate["timestamp"] = json!(event["timestamp"].as_i64().unwrap().to_string());
        let mut history = duplicate.clone();
        history["timestamp"] = json!(1000);
        let (cached, incoming) = reconcile_legacy(
            vec![history.clone(), duplicate.clone()],
            vec![event.clone()],
        );
        assert_eq!(cached, vec![history]);
        assert_eq!(incoming, vec![event.clone()]);
        assert!(reconcile_legacy(vec![event], vec![duplicate]).1.is_empty());
    }
}
