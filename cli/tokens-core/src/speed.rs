//! Inference speed is independent of model identity, reasoning effort and price.
//! Missing evidence stays unknown; a selected tier is not proof of service delivered.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Speed {
    Standard,
    Fast,
    Ultrafast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpeedSource {
    Reported,
    Selected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceSpeed {
    pub speed: Speed,
    pub source: SpeedSource,
}

impl InferenceSpeed {
    /// Anthropic's response usage.speed, NOT usage.service_tier (Priority Tier).
    /// https://platform.claude.com/docs/en/build-with-claude/fast-mode
    pub(crate) fn claude(raw: Option<&str>) -> Option<Self> {
        let speed = match raw? {
            "standard" => Speed::Standard,
            "fast" => Speed::Fast,
            _ => return None,
        };
        Some(Self {
            speed,
            source: SpeedSource::Reported,
        })
    }

    /// Codex's explicit thread preference. Missing/auto/flex are not Standard,
    /// and requests can be downgraded, so never label this provider-reported.
    pub(crate) fn codex(raw: Option<&str>) -> Option<Self> {
        let speed = match raw? {
            "default" => Speed::Standard,
            "fast" | "priority" => Speed::Fast,
            "ultrafast" => Speed::Ultrafast,
            _ => return None,
        };
        Some(Self {
            speed,
            source: SpeedSource::Selected,
        })
    }
}

/// Known-speed counters within a client/model's existing totals. The remainder
/// is unknown. No cost is attributed here: speed recognition does not reprice usage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeedUsage {
    pub speed: Speed,
    pub source: SpeedSource,
    pub tokens: i64,
    pub messages: i32,
}

impl SpeedUsage {
    pub(crate) fn add_to(self, buckets: &mut Vec<Self>) {
        if let Some(bucket) = buckets
            .iter_mut()
            .find(|bucket| bucket.speed == self.speed && bucket.source == self.source)
        {
            bucket.tokens = bucket.tokens.saturating_add(self.tokens);
            bucket.messages = bucket.messages.saturating_add(self.messages);
        } else {
            buckets.push(self);
        }
    }
}
