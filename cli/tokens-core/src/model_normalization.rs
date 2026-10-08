//! Opt-in prefix removal for model IDs in reports and submissions.

use std::sync::OnceLock;

static PREFIXES: OnceLock<Vec<String>> = OnceLock::new();

/// Install user-configured prefixes before model aliases or scans are loaded.
/// The first call wins. An empty list preserves all model prefixes.
pub fn set_prefixes_to_strip(prefixes: &[String]) {
    let _ = PREFIXES.set(
        prefixes
            .iter()
            .filter(|prefix| !prefix.is_empty())
            .map(|prefix| prefix.to_lowercase())
            .collect(),
    );
}

/// Strip the longest configured prefix once, without producing an empty ID.
/// The caller must lowercase the model ID before calling this function.
pub(crate) fn strip_prefix(model_id: &str) -> Option<&str> {
    PREFIXES
        .get()
        .into_iter()
        .flatten()
        .filter_map(|prefix| model_id.strip_prefix(prefix.as_str()))
        .filter(|model| !model.is_empty())
        .min_by_key(|model| model.len())
}

#[cfg(test)]
mod tests {
    #[test]
    fn unconfigured_prefixes_are_preserved() {
        assert_eq!(
            crate::canonical_model_id("gateway-a/gpt-5.4"),
            "gateway-a/gpt-5.4"
        );
        assert_eq!(
            crate::canonical_model_id("gateway-b/claude-opus-4-6"),
            "gateway-b/claude-opus-4-6"
        );
    }
}
