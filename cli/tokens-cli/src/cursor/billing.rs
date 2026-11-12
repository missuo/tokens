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
/// The longest window the API accepts.
const WINDOW_DAYS: i64 = 31;
/// A new cache walks windows back from now until this many consecutive windows
/// return no items, so a month without usage does not end the history early.
const BOOTSTRAP_EMPTY_WINDOWS: usize = 3;
/// Hard ceiling on the windows a bootstrap walks back, about three years.
const BOOTSTRAP_MAX_WINDOWS: usize = 36;
/// How far before the earliest billing row a legacy row is still treated as
/// the same usage. Covers any difference between the two endpoints' event
/// times; dropping a legacy row is preferred to counting its usage twice.
const LEGACY_OVERLAP_MS: i64 = 60 * 60 * 1000;

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
            .map(|model| normalize_model(&model))
            .filter(|model| !model.is_empty())
            .context("Cursor billing item has no model")?;
        ensure!(
            !self.billing_item_id.trim().is_empty(),
            "Cursor billing item has no ID"
        );
        // No cost is carried. The item's only amount is what was charged, which
        // is zero for plan-included usage; the legacy rows' metered
        // `totalCents` has no billing counterpart, so the row is priced from
        // its model instead.
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

/// Billing reports display names (`Claude 4.5 Sonnet (Thinking)`,
/// `Composer 2.5 Fast`) where the legacy endpoint and the pricing tables use
/// ids (`claude-4.5-sonnet-thinking`, `composer-2.5-fast`). Lowercase the name
/// and join its words with `-`; an id passes through unchanged.
fn normalize_model(model: &str) -> String {
    model
        .to_lowercase()
        .split(|ch: char| ch.is_whitespace() || ch == '(' || ch == ')')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// The two APIs report the same usage under different event IDs and, at times,
/// different model names, so no per-row match can tell a legacy row from its
/// billing twin. Billing is authoritative from its earliest row on: every
/// legacy row from shortly before that point onwards is dropped, and only the
/// legacy history that precedes billing survives.
pub(super) fn reconcile_legacy(
    mut cached: Vec<Value>,
    mut incoming: Vec<Value>,
) -> (Vec<Value>, Vec<Value>) {
    let Some(billing_start) = cached
        .iter()
        .chain(&incoming)
        .filter(|event| is_billing_event(event))
        .filter_map(timestamp_ms)
        .min()
    else {
        return (cached, incoming);
    };
    let cutoff = billing_start.saturating_sub(LEGACY_OVERLAP_MS);
    let keep = |event: &Value| {
        is_billing_event(event) || timestamp_ms(event).is_some_and(|ms| ms < cutoff)
    };
    cached.retain(keep);
    incoming.retain(keep);
    (cached, incoming)
}

fn timestamp_ms(event: &Value) -> Option<i64> {
    let value = event.get("timestamp")?;
    value
        .as_i64()
        .or_else(|| value.as_str()?.trim().parse().ok())
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
    let mut windows = Windows::new(since.and_then(DateTime::from_timestamp_millis), Utc::now());
    let mut window_items = 0;
    let deadline = Instant::now() + timeout.unwrap_or(CURSOR_HTTP_TIMEOUT);
    let mut remaining_bytes = CURSOR_MAX_JSON_BYTES;
    let mut events = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..CURSOR_MAX_JSON_PAGES {
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
                    windows.start.to_rfc3339_opts(SecondsFormat::Millis, true),
                ),
                (
                    "endTime",
                    windows.end.to_rfc3339_opts(SecondsFormat::Millis, true),
                ),
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
        window_items += page.data.len();
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
            continue;
        }
        cursor = None;
        if !windows.advance(window_items) {
            return Ok(json!({"usageEventsDisplay": events}).to_string());
        }
        window_items = 0;
    }
    anyhow::bail!("Cursor billing pagination exceeded the request limit")
}

/// The time windows one fetch requests, each at most [`WINDOW_DAYS`] long.
///
/// An incremental sync walks forward from `since` to now. A new cache walks
/// back from now until [`BOOTSTRAP_EMPTY_WINDOWS`] windows in a row hold no
/// items, or [`BOOTSTRAP_MAX_WINDOWS`] have been read, so its first sync holds
/// the account's history rather than only the latest window.
struct Windows {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    now: DateTime<Utc>,
    bootstrap: bool,
    walked: usize,
    empty_in_a_row: usize,
}

impl Windows {
    fn new(since: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Self {
        let window = Duration::days(WINDOW_DAYS);
        let (start, end) = match since {
            Some(since) => (since, (since + window).min(now)),
            None => (now - window, now),
        };
        Self {
            start,
            end,
            now,
            bootstrap: since.is_none(),
            walked: 1,
            empty_in_a_row: 0,
        }
    }

    /// Move to the next window after the current one returned `items` items
    /// (all of them, not only token usage). False when the walk is complete.
    fn advance(&mut self, items: usize) -> bool {
        let window = Duration::days(WINDOW_DAYS);
        if self.bootstrap {
            self.empty_in_a_row = if items == 0 {
                self.empty_in_a_row + 1
            } else {
                0
            };
            if self.empty_in_a_row >= BOOTSTRAP_EMPTY_WINDOWS
                || self.walked >= BOOTSTRAP_MAX_WINDOWS
            {
                return false;
            }
            self.end = self.start;
            self.start = self.end - window;
        } else {
            if self.end >= self.now {
                return false;
            }
            self.start = self.end;
            self.end = (self.start + window).min(self.now);
        }
        self.walked += 1;
        true
    }
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

    fn item(model: &str) -> Value {
        json!({
            "billingItemId": "fixture",
            "eventTime": "2026-11-01T06:05:09.180Z",
            "product": "cursor",
            "chargeCategory": "usage",
            "usage": {"prompt-text-tokens": "1000", "completion-text-tokens": "200"},
            "displayExtras": {"model": model},
            "pricing": {"currency": "USD", "contracted": "0"},
        })
    }

    fn map(raw: Value) -> Value {
        serde_json::from_value::<BillingItem>(raw)
            .unwrap()
            .into_event()
            .unwrap()
            .unwrap()
    }

    fn legacy(ts: i64, model: &str) -> Value {
        json!({
            "usageEventId": format!("legacy-{ts}"),
            "timestamp": ts.to_string(),
            "model": model,
            "tokenUsage": {"inputTokens": 1000, "outputTokens": 200, "totalCents": 1.5},
        })
    }

    /// Display names become the ids the legacy rows and the pricing tables use,
    /// so a billing row is priced from its model rather than falling out of
    /// pricing at $0.
    #[test]
    fn billing_models_are_normalized_to_pricing_ids() {
        for (display, id) in [
            ("Composer 2.5 Fast", "composer-2.5-fast"),
            ("GPT-5.3 Codex", "gpt-5.3-codex"),
            ("Claude 4.5 Sonnet (Thinking)", "claude-4.5-sonnet-thinking"),
            ("claude-4.5-sonnet-thinking", "claude-4.5-sonnet-thinking"),
            ("Auto", "auto"),
            ("  composer-2  ", "composer-2"),
        ] {
            assert_eq!(map(item(display))["model"], id, "{display}");
        }
        assert!(
            serde_json::from_value::<BillingItem>(item("  "))
                .unwrap()
                .into_event()
                .is_err()
        );

        let pricing =
            tokens_core::pricing::PricingService::new(Default::default(), Default::default());
        assert!(
            pricing
                .lookup_with_source("Composer 2.5 Fast", None)
                .is_none(),
            "the raw display name is not a pricing id"
        );
        for display in ["Composer 2.5 Fast", "GPT-5.3 Codex"] {
            let model = map(item(display))["model"].as_str().unwrap().to_owned();
            assert!(
                pricing.calculate_cost(&model, 1000, 200, 0, 0, 0) > 0.0,
                "{model} is priced"
            );
        }
    }

    /// The charged amount is zero for plan-included usage, so it is never
    /// carried as the row's cost; the row is priced like a legacy row without
    /// metered cents.
    #[test]
    fn billing_rows_carry_no_charged_amount_as_cost() {
        let mut raw = item("composer-2");
        raw["pricing"]["contracted"] = json!("0.42");
        let event = map(raw);
        assert!(event.get("chargedCents").is_none());
        assert!(event["tokenUsage"].get("totalCents").is_none());
    }

    /// The same usage can carry different model names on the two endpoints and
    /// event times a little apart, so a legacy row is dropped by time alone.
    #[test]
    fn legacy_rows_inside_billing_coverage_are_dropped_despite_a_model_mismatch() {
        let billing = map(item("Claude 4.5 Sonnet"));
        let ts = billing["timestamp"].as_i64().unwrap();
        let twin = legacy(ts - 2_000, "claude-4.5-sonnet-thinking");
        let later = legacy(ts + 86_400_000, "default");
        let before = legacy(ts - 2 * LEGACY_OVERLAP_MS, "claude-4.5-sonnet-thinking");

        let (cached, incoming) = reconcile_legacy(
            vec![before.clone(), twin.clone(), later.clone()],
            vec![billing.clone()],
        );
        assert_eq!(cached, vec![before.clone()]);
        assert_eq!(incoming, vec![billing.clone()]);

        // A legacy fetch after the account moved to billing adds nothing.
        let (cached, incoming) =
            reconcile_legacy(vec![billing.clone()], vec![twin, later, before.clone()]);
        assert_eq!(cached, vec![billing]);
        assert_eq!(incoming, vec![before]);
    }

    #[test]
    fn a_new_cache_walks_back_until_the_history_is_exhausted() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let window = Duration::days(WINDOW_DAYS);
        let mut walk = Windows::new(None, now);
        assert_eq!((walk.start, walk.end), (now - window, now));

        // Usage, one empty month, more usage, then nothing for three windows.
        let mut starts = vec![walk.start];
        for items in [5, 0, 7, 0, 0] {
            assert!(walk.advance(items));
            starts.push(walk.start);
        }
        assert!(
            !walk.advance(0),
            "three empty windows in a row end the walk"
        );
        assert_eq!(starts.len(), 6);
        for (n, start) in starts.iter().enumerate() {
            assert_eq!(*start, now - window * (n as i32 + 1));
        }

        let mut capped = Windows::new(None, now);
        let mut walked = 1;
        while capped.advance(1) {
            walked += 1;
        }
        assert_eq!(walked, BOOTSTRAP_MAX_WINDOWS);
    }

    #[test]
    fn an_incremental_sync_walks_forward_to_now() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let since = now - Duration::days(40);
        let mut walk = Windows::new(Some(since), now);
        assert_eq!(walk.start, since);
        assert_eq!(walk.end, since + Duration::days(WINDOW_DAYS));
        assert!(
            walk.advance(0),
            "an empty window does not end a forward walk"
        );
        assert_eq!(
            (walk.start, walk.end),
            (since + Duration::days(WINDOW_DAYS), now)
        );
        assert!(!walk.advance(3));
    }
}
