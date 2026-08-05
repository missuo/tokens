fn canonicalize_provider_segment(segment: &str) -> Option<String> {
    let normalized = segment
        .trim()
        .trim_end_matches('/')
        .to_lowercase()
        .replace('-', "_");
    if normalized.starts_with('<') && normalized.ends_with('>') {
        return None;
    }

    let canonical = match normalized.as_str() {
        "" | "unknown" => return None,
        "x_ai" | "xai" => "xai",
        "z_ai" | "zai" => "zai",
        "moonshot" | "moonshotai" => "moonshotai",
        "meta" | "meta_llama" => "meta_llama",
        "azure" | "azure_ai" => "azure_ai",
        "anthropic" | "vertex" | "vertex_ai" => "anthropic",
        "together" | "together_ai" => "together_ai",
        "fireworks" | "fireworks_ai" => "fireworks_ai",
        "google" | "gemini" => "google",
        "openai" | "openai_codex" => "openai",
        "minimax" | "minimaxai" | "minimax_ai" => "minimax",
        "mistral" | "mistralai" => "mistralai",
        "ai21" => "ai21",
        // For unknown segments, reject if they contain digits — those are
        // almost certainly model-name fragments (e.g., "gpt-4", "claude-3")
        // rather than provider identifiers.
        other if other.chars().any(|ch| ch.is_ascii_digit()) => return None,
        other => other,
    };

    Some(canonical.into())
}

pub fn canonical_provider(raw: &str) -> Option<String> {
    provider_tags(raw).into_iter().next()
}

pub fn provider_tags(raw: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut push = |segment: &str| {
        if let Some(tag) = canonicalize_provider_segment(segment) {
            if !tags.iter().any(|existing| existing == &tag) {
                tags.push(tag);
            }
        }
    };

    for segment in raw.trim().trim_end_matches('/').split('/') {
        push(segment);
        if segment.contains('.') {
            for dotted in segment.split('.') {
                push(dotted);
            }
        }
    }

    tags
}

pub fn key_provider_tags(dataset_key: &str) -> Vec<String> {
    let key_parts: Vec<&str> = dataset_key.split('/').collect();
    if key_parts.len() < 2 {
        return Vec::new();
    }

    let mut tags = Vec::new();
    let mut push_all = |value: &str| {
        for tag in provider_tags(value) {
            if !tags.iter().any(|existing| existing == &tag) {
                tags.push(tag);
            }
        }
    };

    for segment in &key_parts[..key_parts.len() - 1] {
        push_all(segment);
    }
    for dotted in key_parts[key_parts.len() - 1].split('.') {
        push_all(dotted);
    }

    tags
}

pub fn matches_provider_hint(dataset_key: &str, provider_id: Option<&str>) -> bool {
    let Some(provider_id) = provider_id else {
        return false;
    };

    let hint_tags = provider_tags(provider_id);
    matches_provider_hint_with_tags(dataset_key, &hint_tags)
}

pub fn matches_provider_hint_with_tags(dataset_key: &str, hint_tags: &[String]) -> bool {
    if hint_tags.is_empty() {
        return false;
    }

    let key_tags = key_provider_tags(dataset_key);
    if key_tags.is_empty() {
        return false;
    }

    key_tags
        .iter()
        .any(|key_tag| hint_tags.iter().any(|hint_tag| hint_tag == key_tag))
}

fn contains_delimited(haystack: &str, needle: &str) -> bool {
    for (pos, _) in haystack.match_indices(needle) {
        let before_ok = pos == 0 || !haystack.as_bytes()[pos - 1].is_ascii_alphanumeric();
        let after_pos = pos + needle.len();
        let after_ok =
            after_pos == haystack.len() || !haystack.as_bytes()[after_pos].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

pub fn inferred_provider_from_model(model: &str) -> Option<&'static str> {
    let lower = model.to_lowercase();

    if lower.contains("claude")
        || lower.contains("anthropic")
        || contains_delimited(&lower, "opus")
        || contains_delimited(&lower, "sonnet")
        || contains_delimited(&lower, "haiku")
        || contains_delimited(&lower, "fable")
    {
        return Some("anthropic");
    }

    if lower.contains("gpt")
        || lower.contains("openai")
        || contains_delimited(&lower, "o1")
        || contains_delimited(&lower, "o3")
        || contains_delimited(&lower, "o4")
    {
        return Some("openai");
    }

    if lower.contains("gemini") || lower.contains("google") {
        return Some("google");
    }

    if lower.contains("grok") {
        return Some("xai");
    }

    if lower.contains("deepseek") {
        return Some("deepseek");
    }

    if lower.contains("minimax") {
        return Some("minimax");
    }

    if lower.contains("mistral") || lower.contains("mixtral") {
        return Some("mistral");
    }

    if lower.contains("llama") || contains_delimited(&lower, "meta") {
        return Some("meta");
    }

    if lower.contains("qwen") {
        return Some("qwen");
    }

    // Sakana's `fugu` / `fugu-ultra` model line. Bare `fugu` is intentionally
    // still mapped to the sakana provider here (provider identity is independent
    // of whether we can price the model — see build_sakana_overrides, which
    // deliberately does NOT price bare `fugu`).
    if lower.contains("fugu") {
        return Some("sakana");
    }

    // Kimi / Moonshot AI — `kimi-k2.5`, `kimi-code`, `moonshot-v1`, etc.
    if contains_delimited(&lower, "kimi") || lower.contains("moonshot") {
        return Some("moonshotai");
    }
    // MiMo (Xiaomi) — `mimo-v2.5` etc.
    if contains_delimited(&lower, "mimo") {
        return Some("xiaomi");
    }
    // GLM (Zhipu AI / Zai) — `glm-4.6`, `glm-5.2` etc.
    if contains_delimited(&lower, "glm") {
        return Some("zai");
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_moonshot_provider_from_model_family() {
        assert_eq!(
            inferred_provider_from_model("moonshot-v1"),
            Some("moonshotai")
        );
        assert_eq!(
            inferred_provider_from_model("MoonshotAI/moonshot-v1-128k"),
            Some("moonshotai")
        );
    }
}
