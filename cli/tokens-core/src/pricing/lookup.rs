use super::{aliases, litellm::ModelPricing};
use crate::{provider_identity, strip_parenthesized_reasoning_tier, TokenBreakdown};
use std::collections::HashMap;
use std::sync::RwLock;

const PROVIDER_PREFIXES: &[&str] = &[
    "openai/",
    "anthropic/",
    "google/",
    "meta-llama/",
    "mistralai/",
    "minimax/",
    "deepseek/",
    "qwen/",
    "cohere/",
    "perplexity/",
    "x-ai/",
];

const ORIGINAL_PROVIDER_PREFIXES: &[&str] = &[
    "x-ai/",
    "xai/",
    "anthropic/",
    "openai/",
    "google/",
    "meta-llama/",
    "mistralai/",
    "minimax/",
    "deepseek/",
    "z-ai/",
    "qwen/",
    "cohere/",
    "perplexity/",
    "moonshotai/",
];

const RESELLER_PROVIDER_PREFIXES: &[&str] = &[
    "azure/",
    "azure_ai/",
    "bedrock/",
    "vertex_ai/",
    "together/",
    "together_ai/",
    "fireworks_ai/",
    "groq/",
    "openrouter/",
];

// Bare brand tokens ("claude", "anthropic") are blocked because they contain
// no model information: a fuzzy hit from them can land on any model of the
// brand (e.g. retired `claude-2.1` eroding to `claude` and billing at an
// opus-fast key), so such a match is never trustworthy.
//
// Generic English words ("model", "router") are blocked for the same reason:
// they carry no model identity, yet substring-match real priced keys
// (`azure_ai/model_router`, `kilo/switchpoint/router`). Without this guard an
// id whose only fuzzy-eligible remnant after suffix stripping is the word
// `model` (e.g. `model-zero-usage-v1` -> stripped `model`) misprices at the
// router key's rate. See `fuzzy_match_does_not_resolve_generic_model_token`.
const FUZZY_BLOCKLIST: &[&str] = &[
    "auto",
    "mini",
    "chat",
    "base",
    "claude",
    "anthropic",
    "model",
    "router",
];

const MAX_LOOKUP_CACHE_ENTRIES: usize = 512;
const TIERED_PRICING_THRESHOLD_128K_TOKENS: f64 = 128_000.0;
const TIERED_PRICING_THRESHOLD_200K_TOKENS: f64 = 200_000.0;
const TIERED_PRICING_THRESHOLD_256K_TOKENS: f64 = 256_000.0;
const TIERED_PRICING_THRESHOLD_272K_TOKENS: f64 = 272_000.0;

const MIN_FUZZY_MATCH_LEN: usize = 5;

/// Minimum length for a model name candidate after prefix/suffix stripping.
/// Prevents false positives like "pro" or "flash" being matched alone.
const MIN_MODEL_NAME_LEN: usize = 2;

/// Maximum number of leading segments that can be treated as a routing prefix.
/// Limits how aggressively we strip (e.g., "a-b-claude-3" strips at most "a-b-").
const MAX_PREFIX_STRIP_SEGMENTS: usize = 2;

/// Maximum number of trailing segments that can be treated as a routing suffix.
/// Handles tier suffixes (-high, -low) and variant suffixes (-thinking, -codex, -codex-max-xhigh).
const MAX_SUFFIX_STRIP_SEGMENTS: usize = 4;

#[derive(Clone)]
struct CachedResult {
    pricing: ModelPricing,
    source: String,
    matched_key: String,
}

struct KeyModelPart {
    key: String,
    lower_model_part: String,
}

struct ProviderScopedModelPath<'a> {
    provider: &'a str,
    terminal_model_id: &'a str,
}

pub struct PricingLookup {
    litellm: HashMap<String, ModelPricing>,
    openrouter: HashMap<String, ModelPricing>,
    cursor: HashMap<String, ModelPricing>,
    sakana: HashMap<String, ModelPricing>,
    models_dev: HashMap<String, ModelPricing>,
    litellm_keys: Vec<String>,
    openrouter_keys: Vec<String>,
    litellm_key_parts: Vec<KeyModelPart>,
    openrouter_key_parts: Vec<KeyModelPart>,
    models_dev_key_parts: Vec<KeyModelPart>,
    litellm_lower: HashMap<String, String>,
    openrouter_lower: HashMap<String, String>,
    models_dev_lower: HashMap<String, String>,
    openrouter_model_part: HashMap<String, String>,
    models_dev_model_part: HashMap<String, String>,
    cursor_lower: HashMap<String, String>,
    sakana_lower: HashMap<String, String>,
    lookup_cache: RwLock<HashMap<String, Option<CachedResult>>>,
}

pub struct LookupResult {
    pub pricing: ModelPricing,
    pub source: String,
    pub matched_key: String,
}

impl PricingLookup {
    pub fn new(
        litellm: HashMap<String, ModelPricing>,
        openrouter: HashMap<String, ModelPricing>,
        cursor: HashMap<String, ModelPricing>,
    ) -> Self {
        // Bare `new` keeps the legacy 3-source shape (no Sakana built-in
        // overrides); production wiring goes through `new_with_models_dev`
        // which threads the Sakana map alongside Cursor.
        Self::new_with_models_dev(litellm, openrouter, cursor, HashMap::new(), HashMap::new())
    }

    pub fn new_with_models_dev(
        litellm: HashMap<String, ModelPricing>,
        openrouter: HashMap<String, ModelPricing>,
        cursor: HashMap<String, ModelPricing>,
        sakana: HashMap<String, ModelPricing>,
        models_dev: HashMap<String, ModelPricing>,
    ) -> Self {
        let mut litellm_keys: Vec<String> = litellm.keys().cloned().collect();
        litellm_keys.sort_by_key(|k| std::cmp::Reverse(k.len()));

        let mut openrouter_keys: Vec<String> = openrouter.keys().cloned().collect();
        openrouter_keys.sort_by_key(|k| std::cmp::Reverse(k.len()));

        let mut models_dev_keys: Vec<String> = models_dev.keys().cloned().collect();
        models_dev_keys.sort_by_key(|k| std::cmp::Reverse(k.len()));

        let mut litellm_lower = HashMap::with_capacity(litellm.len());
        for key in &litellm_keys {
            litellm_lower.insert(key.to_lowercase(), key.clone());
        }

        let mut openrouter_lower = HashMap::with_capacity(openrouter.len());
        let mut openrouter_model_part = HashMap::with_capacity(openrouter.len());
        for key in &openrouter_keys {
            let lower = key.to_lowercase();
            openrouter_lower.insert(lower.clone(), key.clone());
            if let Some(model_part) = lower.split('/').next_back() {
                if model_part != lower {
                    openrouter_model_part.insert(model_part.to_string(), key.clone());
                }
            }
        }

        let mut models_dev_lower = HashMap::with_capacity(models_dev.len());
        let mut models_dev_model_part: HashMap<String, String> =
            HashMap::with_capacity(models_dev.len());
        for key in &models_dev_keys {
            let lower = key.to_lowercase();
            models_dev_lower.insert(lower.clone(), key.clone());
            // Only priced entries enter the model-part index: the
            // deterministic anthropic-first preference must choose among
            // keys that can actually price usage, otherwise an unpriced
            // `anthropic/<model>` row would shadow a priced reseller row
            // and bill the model at zero cost. (The models.dev loader only
            // emits entries with input+output costs — see
            // `models_dev::cost_to_pricing` — but this constructor is
            // public, so the index guards itself too.)
            if !models_dev.get(key).is_some_and(has_any_usable_pricing) {
                continue;
            }
            if let Some(model_part) = lower.split('/').next_back() {
                if model_part != lower {
                    match models_dev_model_part.entry(model_part.to_string()) {
                        std::collections::hash_map::Entry::Occupied(mut entry) => {
                            if prefers_model_part_key(key, entry.get()) {
                                entry.insert(key.clone());
                            }
                        }
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(key.clone());
                        }
                    }
                }
            }
        }

        let mut cursor_lower = HashMap::with_capacity(cursor.len());
        for key in cursor.keys() {
            cursor_lower.insert(key.to_lowercase(), key.clone());
        }

        let mut sakana_lower = HashMap::with_capacity(sakana.len());
        for key in sakana.keys() {
            sakana_lower.insert(key.to_lowercase(), key.clone());
        }

        let build_key_parts = |keys: &[String]| -> Vec<KeyModelPart> {
            keys.iter()
                .map(|key| {
                    let lower = key.to_lowercase();
                    let model_part = lower.split('/').next_back().unwrap_or(&lower).to_string();
                    KeyModelPart {
                        key: key.clone(),
                        lower_model_part: model_part,
                    }
                })
                .collect()
        };

        let litellm_key_parts = build_key_parts(&litellm_keys);
        let openrouter_key_parts = build_key_parts(&openrouter_keys);
        let models_dev_key_parts = build_key_parts(&models_dev_keys);

        Self {
            litellm,
            openrouter,
            cursor,
            sakana,
            models_dev,
            litellm_keys,
            openrouter_keys,
            litellm_key_parts,
            openrouter_key_parts,
            models_dev_key_parts,
            litellm_lower,
            openrouter_lower,
            models_dev_lower,
            openrouter_model_part,
            models_dev_model_part,
            cursor_lower,
            sakana_lower,
            lookup_cache: RwLock::new(HashMap::with_capacity(64)),
        }
    }

    pub fn lookup(&self, model_id: &str) -> Option<LookupResult> {
        self.lookup_with_provider(model_id, None)
    }

    pub fn lookup_with_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        let provider_id = normalize_provider_hint(provider_id);
        let cache_key = build_lookup_cache_key(model_id, provider_id);
        if let Some(cached) = self
            .lookup_cache
            .read()
            .ok()
            .and_then(|c| c.get(&cache_key).cloned())
        {
            return cached.map(|c| LookupResult {
                pricing: c.pricing,
                source: c.source,
                matched_key: c.matched_key,
            });
        }

        let result = self.lookup_with_source_and_provider(model_id, None, provider_id);

        if let Ok(mut cache) = self.lookup_cache.write() {
            if cache.len() >= MAX_LOOKUP_CACHE_ENTRIES {
                // Evict ~25% of entries instead of clearing everything.
                // This avoids a thundering-herd cache miss storm that happens
                // when clear() wipes all entries at once.
                let evict_count = cache.len() / 4;
                let keys_to_remove: Vec<String> = cache.keys().take(evict_count).cloned().collect();
                for key in keys_to_remove {
                    cache.remove(&key);
                }
            }
            cache.insert(
                cache_key,
                result.as_ref().map(|r| CachedResult {
                    pricing: r.pricing.clone(),
                    source: r.source.clone(),
                    matched_key: r.matched_key.clone(),
                }),
            );
        }

        result
    }

    pub fn lookup_with_source(
        &self,
        model_id: &str,
        force_source: Option<&str>,
    ) -> Option<LookupResult> {
        self.lookup_with_source_and_provider(model_id, force_source, None)
    }

    pub fn lookup_with_source_and_provider(
        &self,
        model_id: &str,
        force_source: Option<&str>,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        let provider_id = normalize_provider_hint(provider_id);
        let canonical = aliases::resolve_alias(model_id).unwrap_or(model_id);
        let lower = canonical.to_lowercase();

        // CLIProxyAPI strips `(level)` reasoning-effort suffixes before routing,
        // so for pricing lookup we resolve to the base model regardless of tier.
        // Mirrors the dash-suffix path (e.g. `-xhigh`), which is handled by
        // `try_strip_unknown_suffix` below.
        let normalized_owned = strip_parenthesized_reasoning_tier(&lower).map(str::to_owned);

        // Guard against silent misresolution: if the input ends with `(...)`
        // but the contents are not a recognized CLIProxyAPI level, refuse the
        // lookup. Falling through to `try_strip_unknown_suffix` would split on
        // `-` and could match a shorter, unrelated model id by peeling the
        // parenthesized fragment off (e.g. `gpt-5.2-codex(invalid)` would
        // strip `-codex(invalid)` and resolve to `gpt-5.2`).
        if normalized_owned.is_none()
            && lower
                .strip_suffix(')')
                .and_then(|inner| inner.rsplit_once('('))
                .is_some()
        {
            return None;
        }

        let lower_ref: &str = normalized_owned.as_deref().unwrap_or(&lower);

        // Helper to perform lookup with the given source constraint
        let do_lookup = |id: &str| match force_source {
            Some("litellm") => self.lookup_litellm_only(id, provider_id),
            Some("openrouter") => self.lookup_openrouter_only(id, provider_id),
            Some("models.dev") | Some("modelsdev") | Some("models_dev") => {
                self.lookup_models_dev_only(id, provider_id)
            }
            _ => self.lookup_auto(id, provider_id),
        };
        let requested_family = claude_family(lower_ref);
        let requested_version = requested_claude_version(lower_ref);
        let unparsed_modern_version = requested_family.is_some()
            && requested_version.is_none()
            && contains_delimited_modern_major_minor(lower_ref);
        let unsafe_claude_resolution = |result: &LookupResult| {
            resolves_unsafe_claude_version(
                requested_family,
                requested_version.as_deref(),
                unparsed_modern_version,
                result,
            )
        };

        // 1. Try direct lookup
        if let Some(result) = do_lookup(lower_ref) {
            if unsafe_claude_resolution(&result) {
                return None;
            }
            return Some(result);
        }

        if parse_provider_scoped_model_path(lower_ref).is_some() {
            return None;
        }

        let guarded_lookup = |candidate: &str| {
            do_lookup(candidate).filter(|result| !unsafe_claude_resolution(result))
        };

        // 1.5. Generic provider-routing prefix fallback: ids coming from a
        // router/proxy (e.g. `cx/gpt-5.5` via an `omniroute` provider) carry a
        // prefix outside the curated `PROVIDER_PREFIXES` list, so the
        // known-prefix stripping inside `lookup_auto` never fires for them.
        // The direct exact lookup above already had first crack at the full
        // id, so a dataset key that legitimately keeps its prefix (e.g.
        // `anthropic/claude-fable-5`) resolves there and never reaches this
        // fallback. Only the terminal path segment is retried here, matching
        // the `/`-scoped fallbacks already used by the Cursor/Sakana exact
        // matchers.
        if let Some(terminal) = strip_generic_provider_prefix(lower_ref) {
            if let Some(result) = guarded_lookup(terminal) {
                return Some(result);
            }
        }

        // 2. Try stripping unknown suffixes (e.g., -thinking, -high, -codex)
        if let Some(result) = try_strip_unknown_suffix(lower_ref, guarded_lookup) {
            return Some(result);
        }

        // 3. Try stripping unknown prefixes (e.g., antigravity-, myplugin-)
        //    For each prefix candidate, also try suffix stripping
        if let Some(result) = try_strip_unknown_prefix(lower_ref, guarded_lookup) {
            return Some(result);
        }

        None
    }

    fn lookup_auto(&self, model_id: &str, provider_id: Option<&str>) -> Option<LookupResult> {
        if let Some(result) = self.lookup_provider_scoped_path(model_id, provider_id) {
            return Some(result);
        }
        if parse_provider_scoped_model_path(model_id).is_some() {
            return None;
        }

        if let Some(stripped) = strip_known_provider_prefix(model_id) {
            let prefix_matches_hint =
                provider_id.is_none() || model_prefix_matches_provider(model_id, provider_id);

            if prefix_matches_hint {
                if let Some(exact_litellm) = self.exact_match_litellm(model_id) {
                    return Some(exact_litellm);
                }

                let exact_openrouter = self.exact_match_openrouter(model_id);
                let stripped_litellm = self.exact_or_normalized_litellm(stripped, provider_id);

                if let (Some(litellm), Some(openrouter)) = (&stripped_litellm, &exact_openrouter) {
                    if has_meaningful_tier_support(&litellm.pricing)
                        && !has_any_valid_above_tier_value(&openrouter.pricing)
                    {
                        return stripped_litellm;
                    }
                }

                if let Some(result) = exact_openrouter {
                    return Some(result);
                }
                if let Some(result) = stripped_litellm {
                    return Some(result);
                }
                if let Some(result) = self.exact_match_models_dev(model_id) {
                    return Some(result);
                }
                if let Some(result) =
                    self.exact_match_models_dev_with_provider(stripped, provider_id)
                {
                    return Some(result);
                }
            } else {
                if let Some(result) = choose_best_source_result(
                    self.exact_match_litellm_for_provider(stripped, provider_id),
                    self.exact_match_openrouter_for_provider(stripped, provider_id),
                    provider_id,
                ) {
                    return Some(result);
                }
                if let Some(result) = self.exact_or_normalized_litellm(stripped, provider_id) {
                    return Some(result);
                }
                if let Some(result) =
                    self.exact_match_models_dev_with_provider(stripped, provider_id)
                {
                    return Some(result);
                }
            }
        }

        let exact_litellm = self.exact_match_litellm(model_id);
        if should_prefer_openai_tiered_litellm(model_id, provider_id, exact_litellm.as_ref()) {
            return exact_litellm;
        }

        if let Some(result) = choose_best_source_result(
            self.exact_match_litellm_for_provider(model_id, provider_id),
            self.exact_match_openrouter_for_provider(model_id, provider_id),
            provider_id,
        ) {
            return Some(result);
        }

        if let Some(result) = exact_litellm {
            return Some(result);
        }
        // An unscoped OpenRouter FULL-KEY match is the id's own canonical key,
        // so it wins even under a provider hint. The MODEL-PART fallback does
        // not: it matches "some other provider's model whose model-part equals
        // this id", which is exactly what a provider hint must override.
        if let Some(result) = self.exact_match_openrouter_full_key(model_id) {
            return Some(result);
        }

        // A provider hint pins the lookup to that provider's catalog: the
        // provider-scoped models.dev pass must run before BOTH the unscoped
        // OpenRouter model-part fallback here and the separator-normalized
        // fallback below. Otherwise a hinted lookup (e.g. `venice` + dotted
        // `claude-opus-4.6-fast`, which already matches OpenRouter's
        // `anthropic/claude-opus-4.6-fast` model-part) would take the canonical
        // price instead of the hinted provider's own key. A hint with no
        // matching key falls through to the canonical resolution below.
        if provider_id.is_some() {
            if let Some(result) = self.exact_match_models_dev_for_provider(model_id, provider_id) {
                return Some(result);
            }
        }
        if let Some(result) = self.exact_match_openrouter_model_part(model_id) {
            return Some(result);
        }

        // Separator-normalized exact passes against the canonical sources
        // (LiteLLM + OpenRouter) run BEFORE the models.dev model-part pass so
        // ids like `claude-opus-4-6-fast` hit the canonical
        // `anthropic/claude-opus-4.6-fast` key instead of a reseller's
        // `venice/claude-opus-4-6-fast` markup. models.dev stays the
        // long-tail fallback below. This reorder only preempts models.dev
        // for UNhinted lookups: the provider-scoped passes above and below
        // keep provider-hinted resolutions pinned to the hinted provider.
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) = choose_best_source_result(
                self.exact_match_litellm_for_provider(&version_normalized, provider_id),
                self.exact_match_openrouter_for_provider(&version_normalized, provider_id),
                provider_id,
            ) {
                return Some(result);
            }
            if provider_id.is_some() {
                if let Some(result) =
                    self.exact_match_models_dev_for_provider(&version_normalized, provider_id)
                {
                    return Some(result);
                }
            }
            if let Some(result) = self.exact_match_litellm(&version_normalized) {
                return Some(result);
            }
            if let Some(result) = self.exact_match_openrouter(&version_normalized) {
                return Some(result);
            }
        }

        if let Some(result) = self.exact_match_models_dev_with_provider(model_id, provider_id) {
            return Some(result);
        }
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) =
                self.exact_match_models_dev_with_provider(&version_normalized, provider_id)
            {
                return Some(result);
            }
        }

        if let Some(normalized) = normalize_model_name(model_id) {
            if let Some(result) = choose_best_source_result(
                self.exact_match_litellm_for_provider(&normalized, provider_id),
                self.exact_match_openrouter_for_provider(&normalized, provider_id),
                provider_id,
            ) {
                return Some(result);
            }
            if let Some(result) = self.exact_match_litellm(&normalized) {
                return Some(result);
            }
            if let Some(result) = self.exact_match_openrouter(&normalized) {
                return Some(result);
            }
            if let Some(result) =
                self.exact_match_models_dev_with_provider(&normalized, provider_id)
            {
                return Some(result);
            }
        }

        if let Some(result) = self.prefix_match_litellm(model_id, provider_id) {
            return Some(result);
        }
        if let Some(result) = self.prefix_match_openrouter(model_id, provider_id) {
            return Some(result);
        }
        if let Some(result) = self.prefix_match_models_dev(model_id, provider_id) {
            return Some(result);
        }

        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) = self.prefix_match_litellm(&version_normalized, provider_id) {
                return Some(result);
            }
            if let Some(result) = self.prefix_match_openrouter(&version_normalized, provider_id) {
                return Some(result);
            }
            if let Some(result) = self.prefix_match_models_dev(&version_normalized, provider_id) {
                return Some(result);
            }
        }

        if let Some(result) = self.exact_match_cursor(model_id) {
            return Some(result);
        }
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) = self.exact_match_cursor(&version_normalized) {
                return Some(result);
            }
        }

        // Sakana built-in overrides sit at the SAME precedence as Cursor:
        // upstream real prices (litellm/openrouter/models.dev exact + prefix)
        // already won above, so Sakana only catches ids upstream doesn't price,
        // while still beating the fuzzy guesses below.
        if let Some(result) = self.exact_match_sakana(model_id) {
            return Some(result);
        }
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) = self.exact_match_sakana(&version_normalized) {
                return Some(result);
            }
        }

        if !is_fuzzy_eligible(model_id) {
            return None;
        }

        let litellm_result = self.fuzzy_match_litellm(model_id, provider_id);
        let openrouter_result = self.fuzzy_match_openrouter(model_id, provider_id);

        choose_best_source_result(litellm_result, openrouter_result, provider_id)
    }

    fn exact_or_normalized_litellm(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        if let Some(result) = self.exact_match_litellm_for_provider(model_id, provider_id) {
            return Some(result);
        }
        if let Some(result) = self.exact_match_litellm(model_id) {
            return Some(result);
        }
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) =
                self.exact_match_litellm_for_provider(&version_normalized, provider_id)
            {
                return Some(result);
            }
            if let Some(result) = self.exact_match_litellm(&version_normalized) {
                return Some(result);
            }
        }
        if let Some(normalized) = normalize_model_name(model_id) {
            if let Some(result) = self.exact_match_litellm_for_provider(&normalized, provider_id) {
                return Some(result);
            }
            if let Some(result) = self.exact_match_litellm(&normalized) {
                return Some(result);
            }
        }
        None
    }

    fn lookup_models_dev_only(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        if parse_provider_scoped_model_path(model_id).is_some() {
            return None;
        }

        if let Some(result) = self.exact_match_models_dev_with_provider(model_id, provider_id) {
            return Some(result);
        }
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) =
                self.exact_match_models_dev_with_provider(&version_normalized, provider_id)
            {
                return Some(result);
            }
        }
        if let Some(normalized) = normalize_model_name(model_id) {
            if let Some(result) =
                self.exact_match_models_dev_with_provider(&normalized, provider_id)
            {
                return Some(result);
            }
        }
        if let Some(result) = self.prefix_match_models_dev(model_id, provider_id) {
            return Some(result);
        }
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) = self.prefix_match_models_dev(&version_normalized, provider_id) {
                return Some(result);
            }
        }
        None
    }

    fn lookup_litellm_only(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        if let Some(result) = self.lookup_provider_scoped_path_litellm(model_id, provider_id) {
            return Some(result);
        }
        if parse_provider_scoped_model_path(model_id).is_some() {
            return None;
        }

        if let Some(result) = self.exact_or_normalized_litellm(model_id, provider_id) {
            return Some(result);
        }
        if let Some(stripped) = strip_known_provider_prefix(model_id) {
            if let Some(result) = self.exact_or_normalized_litellm(stripped, provider_id) {
                return Some(result);
            }
        }
        if let Some(result) = self.prefix_match_litellm(model_id, provider_id) {
            return Some(result);
        }
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) = self.prefix_match_litellm(&version_normalized, provider_id) {
                return Some(result);
            }
        }
        if is_fuzzy_eligible(model_id) {
            if let Some(result) = self.fuzzy_match_litellm(model_id, provider_id) {
                return Some(result);
            }
        }
        None
    }

    fn lookup_openrouter_only(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        if let Some(result) = self.lookup_provider_scoped_path_openrouter(model_id, provider_id) {
            return Some(result);
        }
        if parse_provider_scoped_model_path(model_id).is_some() {
            return None;
        }

        if let Some(result) = self.exact_match_openrouter_with_provider(model_id, provider_id) {
            return Some(result);
        }
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) =
                self.exact_match_openrouter_with_provider(&version_normalized, provider_id)
            {
                return Some(result);
            }
        }
        if let Some(normalized) = normalize_model_name(model_id) {
            if let Some(result) =
                self.exact_match_openrouter_with_provider(&normalized, provider_id)
            {
                return Some(result);
            }
        }
        if let Some(result) = self.prefix_match_openrouter(model_id, provider_id) {
            return Some(result);
        }
        if let Some(version_normalized) = normalize_version_separator(model_id) {
            if let Some(result) = self.prefix_match_openrouter(&version_normalized, provider_id) {
                return Some(result);
            }
        }
        if is_fuzzy_eligible(model_id) {
            if let Some(result) = self.fuzzy_match_openrouter(model_id, provider_id) {
                return Some(result);
            }
        }
        None
    }

    fn lookup_provider_scoped_path(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        let scoped = parse_provider_scoped_model_path(model_id)?;
        if !provider_hint_matches_scoped_provider(provider_id, scoped.provider) {
            return None;
        }

        choose_best_source_result(
            self.lookup_provider_scoped_path_litellm(model_id, provider_id),
            self.lookup_provider_scoped_path_openrouter(model_id, provider_id),
            Some(scoped.provider),
        )
    }

    fn lookup_provider_scoped_path_litellm(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        let scoped = parse_provider_scoped_model_path(model_id)?;
        if !provider_hint_matches_scoped_provider(provider_id, scoped.provider) {
            return None;
        }

        if let Some(result) = self.exact_match_litellm(model_id) {
            return Some(result);
        }

        let scoped_tags = provider_identity::provider_tags(scoped.provider);
        for prefix in RESELLER_PROVIDER_PREFIXES {
            if !provider_prefix_matches_scoped_provider(prefix, &scoped_tags) {
                continue;
            }

            let key = format!("{}{}", prefix, model_id);
            if let Some(litellm_key) = self.litellm_lower.get(&key) {
                if let Some(pricing) = self.litellm.get(litellm_key) {
                    if let Some(result) = lookup_result_if_usable(pricing, "LiteLLM", litellm_key) {
                        return Some(result);
                    }
                }
            }
        }

        self.exact_match_litellm_for_provider(scoped.terminal_model_id, Some(scoped.provider))
    }

    fn lookup_provider_scoped_path_openrouter(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        let scoped = parse_provider_scoped_model_path(model_id)?;
        if !provider_hint_matches_scoped_provider(provider_id, scoped.provider) {
            return None;
        }

        self.exact_match_openrouter(model_id).or_else(|| {
            self.exact_match_openrouter_for_provider(
                scoped.terminal_model_id,
                Some(scoped.provider),
            )
        })
    }

    fn exact_match_litellm_for_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        exact_match_with_provider_prefixes(
            model_id,
            provider_id,
            &self.litellm_key_parts,
            &self.litellm,
            "LiteLLM",
        )
    }

    fn exact_match_openrouter_for_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        exact_match_with_provider_prefixes(
            model_id,
            provider_id,
            &self.openrouter_key_parts,
            &self.openrouter,
            "OpenRouter",
        )
    }

    fn exact_match_openrouter_with_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        self.exact_match_openrouter_for_provider(model_id, provider_id)
            .or_else(|| self.exact_match_openrouter(model_id))
    }

    fn exact_match_models_dev_for_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        exact_match_with_provider_prefixes(
            model_id,
            provider_id,
            &self.models_dev_key_parts,
            &self.models_dev,
            "Models.dev",
        )
    }

    fn exact_match_models_dev_with_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        self.exact_match_models_dev_for_provider(model_id, provider_id)
            .or_else(|| self.exact_match_models_dev(model_id))
    }

    fn exact_match_litellm(&self, model_id: &str) -> Option<LookupResult> {
        let key = self.litellm_lower.get(model_id)?;
        let pricing = self.litellm.get(key)?;
        lookup_result_if_usable(pricing, "LiteLLM", key)
    }

    fn exact_match_openrouter(&self, model_id: &str) -> Option<LookupResult> {
        self.exact_match_openrouter_full_key(model_id)
            .or_else(|| self.exact_match_openrouter_model_part(model_id))
    }

    /// Full-key (`provider/model`) exact match against OpenRouter — the id's
    /// own canonical key. This wins even under a provider hint.
    fn exact_match_openrouter_full_key(&self, model_id: &str) -> Option<LookupResult> {
        let key = self.openrouter_lower.get(model_id)?;
        let pricing = self.openrouter.get(key)?;
        lookup_result_if_usable(pricing, "OpenRouter", key)
    }

    /// Model-part exact match against OpenRouter — matches any provider whose
    /// model-part equals `model_id`. A provider hint must take precedence over
    /// this (see `lookup_auto`), otherwise a hinted lookup leaks to a different
    /// provider's canonical key.
    fn exact_match_openrouter_model_part(&self, model_id: &str) -> Option<LookupResult> {
        let key = self.openrouter_model_part.get(model_id)?;
        let pricing = self.openrouter.get(key)?;
        lookup_result_if_usable(pricing, "OpenRouter", key)
    }

    fn exact_match_models_dev(&self, model_id: &str) -> Option<LookupResult> {
        if let Some(key) = self.models_dev_lower.get(model_id) {
            if let Some(pricing) = self.models_dev.get(key) {
                return Some(LookupResult {
                    pricing: pricing.clone(),
                    source: "Models.dev".into(),
                    matched_key: key.clone(),
                });
            }
        }
        if let Some(key) = self.models_dev_model_part.get(model_id) {
            if let Some(pricing) = self.models_dev.get(key) {
                return Some(LookupResult {
                    pricing: pricing.clone(),
                    source: "Models.dev".into(),
                    matched_key: key.clone(),
                });
            }
        }
        None
    }

    fn exact_match_cursor(&self, model_id: &str) -> Option<LookupResult> {
        if let Some(key) = self.cursor_lower.get(model_id) {
            return lookup_result_if_usable(self.cursor.get(key).unwrap(), "Cursor", key);
        }
        if let Some(model_part) = model_id.split('/').next_back() {
            if model_part != model_id {
                if let Some(key) = self.cursor_lower.get(model_part) {
                    return lookup_result_if_usable(self.cursor.get(key).unwrap(), "Cursor", key);
                }
            }
        }
        None
    }

    fn exact_match_sakana(&self, model_id: &str) -> Option<LookupResult> {
        if let Some(key) = self.sakana_lower.get(model_id) {
            return lookup_result_if_usable(self.sakana.get(key).unwrap(), "Sakana", key);
        }
        if let Some(model_part) = model_id.split('/').next_back() {
            if model_part != model_id {
                if let Some(key) = self.sakana_lower.get(model_part) {
                    return lookup_result_if_usable(self.sakana.get(key).unwrap(), "Sakana", key);
                }
            }
        }
        None
    }

    fn prefix_match_litellm(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        if let Some(result) = self.exact_match_litellm_for_provider(model_id, provider_id) {
            return Some(result);
        }

        for prefix in PROVIDER_PREFIXES {
            let key = format!("{}{}", prefix, model_id);
            if let Some(litellm_key) = self.litellm_lower.get(&key) {
                if let Some(pricing) = self.litellm.get(litellm_key) {
                    if let Some(result) = lookup_result_if_usable(pricing, "LiteLLM", litellm_key) {
                        return Some(result);
                    }
                }
            }
        }
        None
    }

    fn prefix_match_openrouter(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        if let Some(result) = self.exact_match_openrouter_for_provider(model_id, provider_id) {
            return Some(result);
        }

        for prefix in PROVIDER_PREFIXES {
            let key = format!("{}{}", prefix, model_id);
            if let Some(or_key) = self.openrouter_lower.get(&key) {
                if let Some(pricing) = self.openrouter.get(or_key) {
                    if let Some(result) = lookup_result_if_usable(pricing, "OpenRouter", or_key) {
                        return Some(result);
                    }
                }
            }
        }
        None
    }

    fn prefix_match_models_dev(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        if let Some(result) = self.exact_match_models_dev_for_provider(model_id, provider_id) {
            return Some(result);
        }

        for prefix in PROVIDER_PREFIXES {
            let key = format!("{}{}", prefix, model_id);
            if let Some(models_dev_key) = self.models_dev_lower.get(&key) {
                if let Some(pricing) = self.models_dev.get(models_dev_key) {
                    return Some(LookupResult {
                        pricing: pricing.clone(),
                        source: "Models.dev".into(),
                        matched_key: models_dev_key.clone(),
                    });
                }
            }
        }
        None
    }

    fn fuzzy_match_litellm(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        let family = extract_model_family(model_id);
        let mut family_matches_list: Vec<&String> = Vec::new();

        for key in &self.litellm_keys {
            let lower_key = key.to_lowercase();
            if family_matches(&lower_key, &family) && contains_model_id(&lower_key, model_id) {
                family_matches_list.push(key);
            }
        }

        if let Some(result) =
            select_best_match(&family_matches_list, &self.litellm, "LiteLLM", provider_id)
        {
            return Some(result);
        }

        let mut all_matches: Vec<&String> = Vec::new();
        for key in &self.litellm_keys {
            let lower_key = key.to_lowercase();
            if contains_model_id(&lower_key, model_id) {
                all_matches.push(key);
            }
        }

        select_best_match(&all_matches, &self.litellm, "LiteLLM", provider_id)
    }

    fn fuzzy_match_openrouter(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
    ) -> Option<LookupResult> {
        let family = extract_model_family(model_id);
        let mut family_matches_list: Vec<&String> = Vec::new();

        for key in &self.openrouter_keys {
            let lower_key = key.to_lowercase();
            let model_part = lower_key.split('/').next_back().unwrap_or(&lower_key);
            if family_matches(model_part, &family) && contains_model_id(model_part, model_id) {
                family_matches_list.push(key);
            }
        }

        if let Some(result) = select_best_match(
            &family_matches_list,
            &self.openrouter,
            "OpenRouter",
            provider_id,
        ) {
            return Some(result);
        }

        let mut all_matches: Vec<&String> = Vec::new();
        for key in &self.openrouter_keys {
            let lower_key = key.to_lowercase();
            let model_part = lower_key.split('/').next_back().unwrap_or(&lower_key);
            if contains_model_id(model_part, model_id) {
                all_matches.push(key);
            }
        }

        select_best_match(&all_matches, &self.openrouter, "OpenRouter", provider_id)
    }

    pub fn calculate_cost(
        &self,
        model_id: &str,
        input: i64,
        output: i64,
        cache_read: i64,
        cache_write: i64,
        reasoning: i64,
    ) -> f64 {
        let usage = TokenBreakdown {
            input,
            output,
            cache_read,
            cache_write,
            reasoning,
        };
        self.calculate_cost_with_provider(model_id, None, &usage)
    }

    pub fn calculate_cost_with_provider(
        &self,
        model_id: &str,
        provider_id: Option<&str>,
        usage: &TokenBreakdown,
    ) -> f64 {
        let provider_id = normalize_provider_hint(provider_id);
        let result = match self.lookup_with_provider(model_id, provider_id) {
            Some(r) => r,
            None => return 0.0,
        };

        compute_cost_for_lookup(&result, provider_id, usage)
    }
}

fn matches_model_or_snapshot(model_id: &str, base: &str) -> bool {
    model_id == base
        || model_id
            .strip_prefix(base)
            .is_some_and(|suffix| suffix.starts_with("-20"))
}

fn is_openai_full_request_272k_model(model_id: &str) -> bool {
    let key = model_id.to_ascii_lowercase();
    let model_id = key.split('/').next_back().unwrap_or(&key);

    [
        "gpt-5.4",
        "gpt-5.4-pro",
        "gpt-5.5",
        // Priced identically to gpt-5.4-pro in LiteLLM ($30/$180 base,
        // $60/$270 above 272k) with the same full-request semantics.
        "gpt-5.5-pro",
        "gpt-5.6",
        "gpt-5.6-sol",
        "gpt-5.6-terra",
        "gpt-5.6-luna",
        // Same full-request 272k semantics as the GPT-5.x flagship family
        // (2x input/cache, 1.5x output for the whole request). Also keeps
        // openai-hinted lookups on LiteLLM's Standard rates instead of
        // OpenRouter's first author endpoint, which is currently Flex (50%).
        // Source: https://developers.openai.com/api/docs/models/gpt-6-astra
        "gpt-6-astra",
        "gpt-6-astra-pro",
    ]
    .into_iter()
    .any(|base| matches_model_or_snapshot(model_id, base))
}

fn should_prefer_openai_tiered_litellm(
    model_id: &str,
    provider_id: Option<&str>,
    litellm: Option<&LookupResult>,
) -> bool {
    provider_id.is_some_and(|provider| {
        provider_identity::canonical_provider(provider).as_deref() == Some("openai")
    }) && is_openai_full_request_272k_model(model_id)
        && litellm.is_some_and(|result| has_complete_openai_272k_pricing(&result.pricing))
}

// A fully-absent cache_read pair used to count as "complete" here (only a
// present-but-partial pair failed), which let the 272k LiteLLM preference
// fire over an OpenRouter entry that actually had cache-read pricing,
// silently dropping it. cache_read is now required present+valid like
// input/output, symmetric with them, for this preference decision only.
fn has_complete_openai_272k_pricing(pricing: &ModelPricing) -> bool {
    let valid_pair = |base: Option<f64>, above: Option<f64>| {
        base.is_some_and(is_valid_price_value) && above.is_some_and(is_valid_price_value)
    };

    valid_pair(
        pricing.input_cost_per_token,
        pricing.input_cost_per_token_above_272k_tokens,
    ) && valid_pair(
        pricing.output_cost_per_token,
        pricing.output_cost_per_token_above_272k_tokens,
    ) && valid_pair(
        pricing.cache_read_input_token_cost,
        pricing.cache_read_input_token_cost_above_272k_tokens,
    )
}

fn uses_openai_full_request_272k_pricing(result: &LookupResult, provider_id: Option<&str>) -> bool {
    if result.source != "LiteLLM"
        || is_reseller_provider(&result.matched_key)
        || provider_id.is_some_and(|provider| {
            provider_identity::canonical_provider(provider).as_deref() != Some("openai")
        })
    {
        return false;
    }

    let key = result.matched_key.to_ascii_lowercase();
    if key.contains('/') && !key.starts_with("openai/") {
        return false;
    }

    is_openai_full_request_272k_model(&key)
}

fn compute_cost_for_lookup(
    result: &LookupResult,
    provider_id: Option<&str>,
    usage: &TokenBreakdown,
) -> f64 {
    let calculate = |pricing| {
        compute_cost(
            pricing,
            usage.input,
            usage.output,
            usage.cache_read,
            usage.cache_write,
            usage.reasoning,
        )
    };
    let total_input = usage
        .input
        .max(0)
        .saturating_add(usage.cache_read.max(0))
        .saturating_add(usage.cache_write.max(0));
    if !uses_openai_full_request_272k_pricing(result, provider_id) {
        return calculate(&result.pricing);
    }

    let mut pricing = result.pricing.clone();
    if total_input <= TIERED_PRICING_THRESHOLD_272K_TOKENS as i64 {
        pricing.input_cost_per_token_above_272k_tokens = None;
        pricing.output_cost_per_token_above_272k_tokens = None;
        pricing.cache_read_input_token_cost_above_272k_tokens = None;
        return calculate(&pricing);
    }

    if let Some(high) = pricing
        .input_cost_per_token_above_272k_tokens
        .filter(|price| is_valid_price_value(*price))
    {
        let input_multiplier = pricing
            .input_cost_per_token
            .filter(|base| is_valid_price_value(*base) && *base > 0.0)
            .map(|base| high / base);
        for rate in [
            &mut pricing.input_cost_per_token,
            &mut pricing.input_cost_per_token_above_128k_tokens,
            &mut pricing.input_cost_per_token_above_200k_tokens,
            &mut pricing.input_cost_per_token_above_256k_tokens,
            &mut pricing.input_cost_per_token_above_272k_tokens,
        ] {
            *rate = Some(high);
        }

        if let (Some(multiplier), Some(cache_write_price)) = (
            input_multiplier,
            pricing
                .cache_creation_input_token_cost
                .filter(|price| is_valid_price_value(*price)),
        ) {
            let high = Some(cache_write_price * multiplier);
            pricing.cache_creation_input_token_cost = high;
            pricing.cache_creation_input_token_cost_above_200k_tokens = high;
        }
    }
    if let Some(high) = pricing
        .output_cost_per_token_above_272k_tokens
        .filter(|price| is_valid_price_value(*price))
    {
        for rate in [
            &mut pricing.output_cost_per_token,
            &mut pricing.output_cost_per_token_above_128k_tokens,
            &mut pricing.output_cost_per_token_above_200k_tokens,
            &mut pricing.output_cost_per_token_above_256k_tokens,
            &mut pricing.output_cost_per_token_above_272k_tokens,
        ] {
            *rate = Some(high);
        }
    }
    if let Some(high) = pricing
        .cache_read_input_token_cost_above_272k_tokens
        .filter(|price| is_valid_price_value(*price))
    {
        for rate in [
            &mut pricing.cache_read_input_token_cost,
            &mut pricing.cache_read_input_token_cost_above_200k_tokens,
            &mut pricing.cache_read_input_token_cost_above_272k_tokens,
        ] {
            *rate = Some(high);
        }
    }

    calculate(&pricing)
}

pub fn compute_cost(
    pricing: &ModelPricing,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
) -> f64 {
    let safe_price = |opt: Option<f64>| opt.filter(|v| is_valid_price_value(*v)).unwrap_or(0.0);
    let tiered_cost = |tokens: f64, base: Option<f64>, tiers: &[(f64, Option<f64>)]| {
        let base_price = safe_price(base);
        let mut cost = 0.0;
        let mut lower_bound = 0.0;
        let mut active_price = base_price;

        for (threshold, tier_price) in tiers {
            let Some(tier_price) = tier_price.filter(|v| is_valid_price_value(*v)) else {
                continue;
            };

            if !threshold.is_finite() || *threshold <= lower_bound {
                continue;
            }

            if tokens <= *threshold {
                return cost + (tokens - lower_bound).max(0.0) * active_price;
            }

            cost += (*threshold - lower_bound) * active_price;
            lower_bound = *threshold;
            active_price = tier_price;
        }

        cost + (tokens - lower_bound).max(0.0) * active_price
    };

    let input_clamped = input.max(0) as f64;
    let output_clamped = output.max(0).saturating_add(reasoning.max(0)) as f64;
    let cache_read_clamped = cache_read.max(0) as f64;
    let cache_write_clamped = cache_write.max(0) as f64;

    let input_cost = tiered_cost(
        input_clamped,
        pricing.input_cost_per_token,
        &[
            (
                TIERED_PRICING_THRESHOLD_128K_TOKENS,
                pricing.input_cost_per_token_above_128k_tokens,
            ),
            (
                TIERED_PRICING_THRESHOLD_200K_TOKENS,
                pricing.input_cost_per_token_above_200k_tokens,
            ),
            (
                TIERED_PRICING_THRESHOLD_256K_TOKENS,
                pricing.input_cost_per_token_above_256k_tokens,
            ),
            (
                TIERED_PRICING_THRESHOLD_272K_TOKENS,
                pricing.input_cost_per_token_above_272k_tokens,
            ),
        ],
    );
    let output_cost = tiered_cost(
        output_clamped,
        pricing.output_cost_per_token,
        &[
            (
                TIERED_PRICING_THRESHOLD_128K_TOKENS,
                pricing.output_cost_per_token_above_128k_tokens,
            ),
            (
                TIERED_PRICING_THRESHOLD_200K_TOKENS,
                pricing.output_cost_per_token_above_200k_tokens,
            ),
            (
                TIERED_PRICING_THRESHOLD_256K_TOKENS,
                pricing.output_cost_per_token_above_256k_tokens,
            ),
            (
                TIERED_PRICING_THRESHOLD_272K_TOKENS,
                pricing.output_cost_per_token_above_272k_tokens,
            ),
        ],
    );
    // Cache-read tiers stay limited to the 200k and 272k thresholds
    // because upstream LiteLLM does not currently declare 128k or 256k
    // cache-read pricing for any model. If upstream begins emitting
    // those keys, also add matching fields to `ModelPricing`,
    // `has_any_usable_pricing`, `has_any_valid_above_tier_value`, and
    // `has_meaningful_tier_support`; otherwise tier walks will silently
    // undercost long-context cache reads on those models.
    let cache_read_cost = tiered_cost(
        cache_read_clamped,
        pricing.cache_read_input_token_cost,
        &[
            (
                TIERED_PRICING_THRESHOLD_200K_TOKENS,
                pricing.cache_read_input_token_cost_above_200k_tokens,
            ),
            (
                TIERED_PRICING_THRESHOLD_272K_TOKENS,
                pricing.cache_read_input_token_cost_above_272k_tokens,
            ),
        ],
    );
    let cache_write_cost = tiered_cost(
        cache_write_clamped,
        pricing.cache_creation_input_token_cost,
        &[(
            TIERED_PRICING_THRESHOLD_200K_TOKENS,
            pricing.cache_creation_input_token_cost_above_200k_tokens,
        )],
    );

    input_cost + output_cost + cache_read_cost + cache_write_cost
}

fn extract_model_family(model_id: &str) -> String {
    let lower = model_id.to_lowercase();

    if lower.contains("gpt-5") {
        return "gpt-5".into();
    }
    if lower.contains("gpt-4.1") {
        return "gpt-4.1".into();
    }
    if lower.contains("gpt-4o") {
        return "gpt-4o".into();
    }
    if lower.contains("gpt-4") {
        return "gpt-4".into();
    }
    if lower.contains("o3") {
        return "o3".into();
    }
    if lower.contains("o4") {
        return "o4".into();
    }

    if lower.contains("opus") {
        return "opus".into();
    }
    if lower.contains("sonnet") {
        return "sonnet".into();
    }
    if lower.contains("haiku") {
        return "haiku".into();
    }
    if lower.contains("claude") {
        return "claude".into();
    }

    if lower.contains("gemini-3") {
        return "gemini-3".into();
    }
    if lower.contains("gemini-2.5") {
        return "gemini-2.5".into();
    }
    if lower.contains("gemini-2") {
        return "gemini-2".into();
    }
    if lower.contains("gemini") {
        return "gemini".into();
    }

    if lower.contains("llama") {
        return "llama".into();
    }
    if lower.contains("mistral") {
        return "mistral".into();
    }
    if lower.contains("deepseek") {
        return "deepseek".into();
    }
    if lower.contains("qwen") {
        return "qwen".into();
    }

    lower
        .split(['-', '_', '.'])
        .next()
        .unwrap_or(&lower)
        .to_string()
}

fn family_matches(key: &str, family: &str) -> bool {
    if family.is_empty() {
        return true;
    }
    key.contains(family)
}

fn contains_model_id(key: &str, model_id: &str) -> bool {
    if let Some(pos) = key.find(model_id) {
        let before_ok = pos == 0 || !key[..pos].chars().last().unwrap().is_alphanumeric();
        let after_pos = pos + model_id.len();
        let after_ok =
            after_pos == key.len() || !key[after_pos..].chars().next().unwrap().is_alphanumeric();
        before_ok && after_ok
    } else {
        false
    }
}

fn normalize_model_name(model_id: &str) -> Option<String> {
    let lower = model_id.to_lowercase();
    let family = claude_family(&lower)?;

    // Modern Claude line (major >= 4): explicit single-digit minor parsed
    // straight from the id, in either order (claude-sonnet-4-6, opus-4.8,
    // claude-4-6-sonnet). New minor releases need no code change.
    if let Some(model) = normalize_claude_family_minor(&lower) {
        return Some(model);
    }

    // Never degrade: a delimited `major(-|.)minor` version whose minor was
    // not recognized above (4-60, 4-0, 5-0, dated 4-20250514) must stay
    // unresolved rather than fall through to a coarser or older key.
    if contains_delimited_modern_major_minor(&lower) {
        return None;
    }

    // Bare modern major adjacent to the family token (claude-sonnet-5,
    // opus-5, 4-opus). Resolves only via an exact dataset hit downstream.
    if let Some(model) = normalize_claude_family_bare_major(&lower) {
        return Some(model);
    }

    // Catch-alls preserved from the hardcoded matcher: a delimited `4`
    // anywhere still maps opus/sonnet to the bare 4.0 key, and the legacy
    // 3.x line uses irregular naming (family after the version, dotted 3.5).
    match family {
        "opus" if contains_delimited_fragment(&lower, "4") => Some("claude-opus-4".into()),
        "sonnet" => {
            if contains_delimited_fragment(&lower, "4") {
                Some("claude-sonnet-4".into())
            } else if contains_delimited_fragment(&lower, "3.7")
                || contains_delimited_fragment(&lower, "3-7")
            {
                Some("claude-3-7-sonnet".into())
            } else if contains_delimited_fragment(&lower, "3.5")
                || contains_delimited_fragment(&lower, "3-5")
            {
                Some("claude-3.5-sonnet".into())
            } else {
                None
            }
        }
        "haiku" => {
            if contains_delimited_fragment(&lower, "3.5")
                || contains_delimited_fragment(&lower, "3-5")
            {
                Some("claude-3.5-haiku".into())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Family tokens of the modern Claude model line.
const CLAUDE_FAMILY_TOKENS: &[&str] = &["opus", "sonnet", "haiku", "fable"];

/// The Claude family token contained in `lower`, if any.
fn claude_family(lower: &str) -> Option<&'static str> {
    CLAUDE_FAMILY_TOKENS
        .iter()
        .copied()
        .find(|family| lower.contains(family))
}

/// Modern Claude majors are single digits >= 4. The 3.x line uses irregular
/// naming and is matched explicitly by the legacy branches.
fn is_modern_claude_major(value: &str) -> bool {
    value.len() == 1 && value.as_bytes()[0].is_ascii_digit() && value.as_bytes()[0] >= b'4'
}

/// Canonical `claude-{family}-{major}-{minor}` key parsed from an id carrying
/// an explicit single-digit minor for a modern major (>= 4), in either
/// `family-major-minor` (claude-sonnet-4-6, opus-4.8) or reversed
/// `major-minor-family` (claude-4-6-sonnet, 4-8-opus) order. Generalization
/// of the former opus-only `normalize_claude_opus_4_minor` across families.
fn normalize_claude_family_minor(lower: &str) -> Option<String> {
    let parts: Vec<&str> = lower
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect();

    for window in parts.windows(3) {
        if CLAUDE_FAMILY_TOKENS.contains(&window[0])
            && is_modern_claude_major(window[1])
            && is_single_digit_minor(window[2])
        {
            return Some(format!("claude-{}-{}-{}", window[0], window[1], window[2]));
        }
        if is_modern_claude_major(window[0])
            && is_single_digit_minor(window[1])
            && CLAUDE_FAMILY_TOKENS.contains(&window[2])
        {
            return Some(format!("claude-{}-{}-{}", window[2], window[0], window[1]));
        }
    }

    None
}

/// Canonical `claude-{family}-{major}` key for an id naming a modern major
/// (>= 4) without a minor (claude-sonnet-5, opus-5, 4-opus). The major must
/// be adjacent to the family token; in forward order it must not be followed
/// by another digit run (dated `4-20250514` shapes are version-like, not
/// bare), and in reversed order it must not itself be the minor of a
/// preceding legacy major (claude-3-5-sonnet).
fn normalize_claude_family_bare_major(lower: &str) -> Option<String> {
    let parts: Vec<&str> = lower
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect();
    let all_digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());

    for (idx, part) in parts.iter().enumerate() {
        if !CLAUDE_FAMILY_TOKENS.contains(part) {
            continue;
        }
        if let Some(major) = parts
            .get(idx + 1)
            .copied()
            .filter(|p| is_modern_claude_major(p))
        {
            if parts.get(idx + 2).is_none_or(|next| !all_digits(next)) {
                return Some(format!("claude-{part}-{major}"));
            }
        }
        if idx >= 1
            && is_modern_claude_major(parts[idx - 1])
            && (idx < 2 || !all_digits(parts[idx - 2]))
        {
            return Some(format!("claude-{part}-{}", parts[idx - 1]));
        }
    }

    None
}

/// True if the id carries a delimited modern `major(-|.)minor` version
/// (4-6, 4.8, 5-0, 4-60, 4-20250514). Generalizes the former
/// `contains_delimited_major_minor(lower, '4')` checks across all modern
/// majors so the never-degrade contract also covers major 5 and up.
fn contains_delimited_modern_major_minor(haystack: &str) -> bool {
    ('4'..='9').any(|major| contains_delimited_major_minor(haystack, major))
}

/// The version-pinned canonical key a Claude id requests, used to veto
/// fuzzy/stripped resolutions that would land on a different version.
///
/// - An explicit single-digit minor (claude-sonnet-4-7) always pins; this is
///   main's opus-only minor guard generalized across families.
/// - A bare major pins from major 5 up (claude-opus-5 must never bill as any
///   opus 4.x key). Bare major 4 is deliberately left unpinned to preserve
///   the long-standing behavior of e.g. `claude-opus-4` resolving to a
///   dated or regional 4.x dataset key.
fn requested_claude_version(lower: &str) -> Option<String> {
    if let Some(model) = normalize_claude_family_minor(lower) {
        return Some(model);
    }
    normalize_claude_family_bare_major(lower).filter(|model| !model.ends_with("-4"))
}

/// Veto for resolutions that violate the never-degrade contract:
/// cross-family (a sonnet id billed at an opus key), cross-version (a 4-7 id
/// billed at a 4-6 key, a major-5 id billed at a 4.x key), or any
/// modern-Claude resolution for an id whose `major-minor` version could not
/// be parsed (4-60, 5-0, dated forms). Exact dataset hits stay allowed: they
/// either normalize back to the requested version or, for unparseable
/// versions, do not normalize at all. Generalization of the former
/// `resolves_different_claude_opus_4_minor`.
fn resolves_unsafe_claude_version(
    requested_family: Option<&'static str>,
    requested_version: Option<&str>,
    unparsed_modern_version: bool,
    result: &LookupResult,
) -> bool {
    let Some(requested_family) = requested_family else {
        return false;
    };
    let matched_lower = result.matched_key.to_lowercase();

    if claude_family(&matched_lower).is_some_and(|family| family != requested_family) {
        return true;
    }

    let resolved = normalize_model_name(&matched_lower);
    if let Some(requested_version) = requested_version {
        return resolved.is_some_and(|resolved| resolved != requested_version);
    }
    unparsed_modern_version && resolved.is_some()
}

fn is_single_digit_minor(value: &str) -> bool {
    value.len() == 1 && value.as_bytes()[0].is_ascii_digit() && value.as_bytes()[0] != b'0'
}

fn normalize_version_separator(model_id: &str) -> Option<String> {
    let mut result = String::with_capacity(model_id.len());
    let chars: Vec<char> = model_id.chars().collect();
    let mut changed = false;

    for i in 0..chars.len() {
        if chars[i] == '-'
            && i > 0
            && i < chars.len() - 1
            && chars[i - 1].is_ascii_digit()
            && chars[i + 1].is_ascii_digit()
        {
            let is_multi_digit_before = i >= 2 && chars[i - 2].is_ascii_digit();
            let is_multi_digit_after = i + 2 < chars.len() && chars[i + 2].is_ascii_digit();
            let looks_like_date = is_multi_digit_before || is_multi_digit_after;

            if looks_like_date {
                result.push(chars[i]);
            } else {
                result.push('.');
                changed = true;
            }
        } else {
            result.push(chars[i]);
        }
    }

    if changed {
        Some(result)
    } else {
        None
    }
}

fn strip_known_provider_prefix(model_id: &str) -> Option<&str> {
    for prefix in PROVIDER_PREFIXES {
        if let Some(stripped) = model_id.strip_prefix(prefix) {
            if !stripped.is_empty() {
                return Some(stripped);
            }
        }
    }
    None
}

/// Generic routing-prefix fallback for ids whose leading segment is not one
/// of the curated `PROVIDER_PREFIXES` (e.g. `cx/gpt-5.5` routed through an
/// `omniroute` proxy, or any other CLI/router-assigned alias). Returns the
/// terminal path segment — the part after the last `/` — when the id
/// actually contains a `/`, so `cx/gpt-5.5` resolves to `gpt-5.5`.
///
/// This is intentionally unconditional (unlike `strip_known_provider_prefix`,
/// which only recognizes canonical LLM provider names): the caller only
/// invokes it as a fallback AFTER the exact/direct lookup on the full id has
/// already failed, so dataset keys that legitimately keep their prefix (e.g.
/// `anthropic/claude-fable-5`) are resolved by their own exact key first and
/// never reach this fallback.
fn strip_generic_provider_prefix(model_id: &str) -> Option<&str> {
    let terminal = model_id.rsplit('/').next()?;
    if terminal.is_empty() || terminal == model_id {
        return None;
    }
    Some(terminal)
}

fn is_valid_price_value(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

/// Returns true if the pricing entry has at least one usable cost field
/// (base or above-200k tier). Entries with all-None pricing (e.g.
/// subscription-based providers like Perplexity) are useless for
/// pay-per-token cost estimation and should be deprioritized.
fn has_any_usable_pricing(pricing: &ModelPricing) -> bool {
    [
        pricing.input_cost_per_token,
        pricing.output_cost_per_token,
        pricing.cache_read_input_token_cost,
        pricing.cache_creation_input_token_cost,
        pricing.input_cost_per_token_above_128k_tokens,
        pricing.input_cost_per_token_above_200k_tokens,
        pricing.input_cost_per_token_above_256k_tokens,
        pricing.input_cost_per_token_above_272k_tokens,
        pricing.output_cost_per_token_above_128k_tokens,
        pricing.output_cost_per_token_above_200k_tokens,
        pricing.output_cost_per_token_above_256k_tokens,
        pricing.output_cost_per_token_above_272k_tokens,
        pricing.cache_read_input_token_cost_above_200k_tokens,
        pricing.cache_read_input_token_cost_above_272k_tokens,
        pricing.cache_creation_input_token_cost_above_200k_tokens,
    ]
    .into_iter()
    .any(|opt| opt.is_some_and(is_valid_price_value))
}

fn lookup_result_if_usable(
    pricing: &ModelPricing,
    source: &str,
    matched_key: &str,
) -> Option<LookupResult> {
    has_any_usable_pricing(pricing).then(|| LookupResult {
        pricing: pricing.clone(),
        source: source.into(),
        matched_key: matched_key.into(),
    })
}

fn has_any_valid_above_tier_value(pricing: &ModelPricing) -> bool {
    [
        pricing.input_cost_per_token_above_128k_tokens,
        pricing.input_cost_per_token_above_200k_tokens,
        pricing.input_cost_per_token_above_256k_tokens,
        pricing.input_cost_per_token_above_272k_tokens,
        pricing.output_cost_per_token_above_128k_tokens,
        pricing.output_cost_per_token_above_200k_tokens,
        pricing.output_cost_per_token_above_256k_tokens,
        pricing.output_cost_per_token_above_272k_tokens,
        pricing.cache_read_input_token_cost_above_200k_tokens,
        pricing.cache_read_input_token_cost_above_272k_tokens,
        pricing.cache_creation_input_token_cost_above_200k_tokens,
    ]
    .into_iter()
    .flatten()
    .any(is_valid_price_value)
}

fn has_meaningful_tier_support(pricing: &ModelPricing) -> bool {
    [
        (
            pricing.input_cost_per_token,
            pricing.input_cost_per_token_above_128k_tokens,
        ),
        (
            pricing.input_cost_per_token,
            pricing.input_cost_per_token_above_200k_tokens,
        ),
        (
            pricing.input_cost_per_token,
            pricing.input_cost_per_token_above_256k_tokens,
        ),
        (
            pricing.input_cost_per_token,
            pricing.input_cost_per_token_above_272k_tokens,
        ),
        (
            pricing.output_cost_per_token,
            pricing.output_cost_per_token_above_128k_tokens,
        ),
        (
            pricing.output_cost_per_token,
            pricing.output_cost_per_token_above_200k_tokens,
        ),
        (
            pricing.output_cost_per_token,
            pricing.output_cost_per_token_above_256k_tokens,
        ),
        (
            pricing.output_cost_per_token,
            pricing.output_cost_per_token_above_272k_tokens,
        ),
    ]
    .into_iter()
    .any(|(base, above)| match (base, above) {
        (Some(base), Some(above)) => base.is_finite() && base >= 0.0 && is_valid_price_value(above),
        _ => false,
    })
}

fn contains_delimited_fragment(haystack: &str, fragment: &str) -> bool {
    if fragment.is_empty() {
        return false;
    }

    for (pos, _) in haystack.match_indices(fragment) {
        let before_ok = pos == 0 || !haystack[..pos].chars().last().unwrap().is_alphanumeric();
        let after_pos = pos + fragment.len();
        let after_ok = after_pos == haystack.len()
            || !haystack[after_pos..]
                .chars()
                .next()
                .unwrap()
                .is_alphanumeric();

        if before_ok && after_ok {
            return true;
        }
    }

    false
}

fn contains_delimited_major_minor(haystack: &str, major: char) -> bool {
    for (pos, _) in haystack.match_indices(major) {
        let before_ok = pos == 0 || !haystack[..pos].chars().last().unwrap().is_alphanumeric();
        let after_pos = pos + major.len_utf8();
        let mut after = haystack[after_pos..].chars();
        let Some(separator) = after.next() else {
            continue;
        };
        let Some(minor_start) = after.next() else {
            continue;
        };

        if before_ok && matches!(separator, '.' | '-') && minor_start.is_ascii_digit() {
            return true;
        }
    }

    false
}

fn is_fuzzy_eligible(model_id: &str) -> bool {
    if model_id.len() < MIN_FUZZY_MATCH_LEN {
        return false;
    }
    !FUZZY_BLOCKLIST.contains(&model_id)
}

/// Attempts to find a model by progressively stripping trailing segments.
/// Handles arbitrary suffixes (e.g., "claude-sonnet-4-5-thinking" → "claude-sonnet-4-5").
/// This replaces the hardcoded TIER_SUFFIXES and FALLBACK_SUFFIXES approach.
fn try_strip_unknown_suffix<F>(model_id: &str, do_lookup: F) -> Option<LookupResult>
where
    F: Fn(&str) -> Option<LookupResult>,
{
    if has_unrecognized_claude_four_minor(model_id) {
        return None;
    }

    let parts: Vec<&str> = model_id.split('-').collect();

    if parts.len() < 2 {
        return None;
    }

    let max_strip = std::cmp::min(parts.len() - 1, MAX_SUFFIX_STRIP_SEGMENTS);

    for strip in 1..=max_strip {
        let candidate: String = parts[..parts.len() - strip].join("-");

        if candidate.len() >= MIN_MODEL_NAME_LEN {
            if strips_claude_numeric_minor(&candidate, parts[parts.len() - strip]) {
                continue;
            }

            if let Some(result) = do_lookup(&candidate) {
                return Some(result);
            }
        }
    }

    None
}

fn strips_claude_numeric_minor(candidate: &str, first_stripped_segment: &str) -> bool {
    if !is_version_segment(first_stripped_segment) {
        return false;
    }
    let claude_branded = candidate.contains("claude")
        || candidate.contains("opus")
        || candidate.contains("sonnet")
        || candidate.contains("haiku");
    if !claude_branded {
        return false;
    }
    // Refuse to strip a version segment when it would either peel a minor off
    // a still-versioned claude-4 candidate (claude-sonnet-4-5 -> claude-sonnet-4)
    // or erode the id's only version, leaving a bare brand token
    // (claude-2.1 -> claude). Both candidates would resolve to a different
    // model's price. Dated forms (claude-3-5-sonnet-20241022) keep stripping:
    // their candidate retains a version, so neither arm fires.
    contains_delimited_fragment(candidate, "4") || !candidate.bytes().any(|b| b.is_ascii_digit())
}

/// True for a bare version segment produced by splitting an id on `-`:
/// digits with at most one interior dot (`4`, `6`, `2.1`, `20241022`).
fn is_version_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_digit() || !bytes[bytes.len() - 1].is_ascii_digit() {
        return false;
    }
    let mut seen_dot = false;
    for &byte in bytes {
        match byte {
            b'0'..=b'9' => {}
            b'.' if !seen_dot => seen_dot = true,
            _ => return false,
        }
    }
    true
}

fn has_unrecognized_claude_four_minor(model_id: &str) -> bool {
    (model_id.contains("claude")
        || model_id.contains("opus")
        || model_id.contains("sonnet")
        || model_id.contains("haiku"))
        && contains_delimited_major_minor(model_id, '4')
        && !contains_delimited_fragment(model_id, "4.5")
        && !contains_delimited_fragment(model_id, "4-5")
        && !contains_delimited_fragment(model_id, "4.6")
        && !contains_delimited_fragment(model_id, "4-6")
        && !contains_delimited_fragment(model_id, "4.7")
        && !contains_delimited_fragment(model_id, "4-7")
}

/// Attempts to find a model by progressively stripping leading segments.
/// Handles arbitrary routing prefixes (e.g., "myplugin-claude-3.5-sonnet" → "claude-3.5-sonnet").
/// This replaces the hardcoded STRIPPED_PREFIXES approach.
fn try_strip_unknown_prefix<F>(model_id: &str, do_lookup: F) -> Option<LookupResult>
where
    F: Fn(&str) -> Option<LookupResult>,
{
    let parts: Vec<&str> = model_id.split('-').collect();

    if parts.len() < 2 {
        return None;
    }

    let max_skip = std::cmp::min(parts.len() - 1, MAX_PREFIX_STRIP_SEGMENTS);

    for skip in 1..=max_skip {
        let candidate: String = parts[skip..].join("-");

        if candidate.len() >= MIN_MODEL_NAME_LEN {
            // Try candidate directly
            if let Some(result) = do_lookup(&candidate) {
                return Some(result);
            }

            // Try candidate with suffix stripping
            if let Some(result) = try_strip_unknown_suffix(&candidate, &do_lookup) {
                return Some(result);
            }
        }
    }

    None
}

/// Deterministic provider choice when multiple models.dev providers share a
/// model part: the canonical `anthropic/` namespace wins outright; otherwise
/// the shorter key is preferred (the historical winner of the insertion-order
/// race, keeping existing resolutions stable), with lexicographic order
/// breaking length ties so the result no longer depends on HashMap iteration
/// order.
fn prefers_model_part_key(candidate: &str, existing: &str) -> bool {
    let candidate_lower = candidate.to_lowercase();
    let existing_lower = existing.to_lowercase();
    let is_anthropic = |key: &str| key.split('/').next() == Some("anthropic");
    match (
        is_anthropic(&candidate_lower),
        is_anthropic(&existing_lower),
    ) {
        (true, false) => true,
        (false, true) => false,
        _ => (candidate_lower.len(), candidate_lower) < (existing_lower.len(), existing_lower),
    }
}

fn is_original_provider(key: &str) -> bool {
    let lower = key.to_lowercase();
    ORIGINAL_PROVIDER_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

fn is_reseller_provider(key: &str) -> bool {
    let lower = key.to_lowercase();
    RESELLER_PROVIDER_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

fn select_best_match(
    matches: &[&String],
    dataset: &HashMap<String, ModelPricing>,
    source: &str,
    provider_id: Option<&str>,
) -> Option<LookupResult> {
    if matches.is_empty() {
        return None;
    }

    let hint_tags: Vec<String> = provider_id
        .map(provider_identity::provider_tags)
        .unwrap_or_default();

    let provider_matches: Vec<&String> = matches
        .iter()
        .copied()
        .filter(|key| provider_identity::matches_provider_hint_with_tags(key, &hint_tags))
        .collect();

    let preferred_matches = if provider_matches.is_empty() {
        matches
    } else {
        provider_matches.as_slice()
    };

    // Deprioritize entries with all-None pricing (e.g. perplexity/anthropic/...
    // which matches provider hint "anthropic" but has subscription-based pricing
    // with no per-token cost data). If provider-specific candidates are all
    // unusable, fall back to any priced candidate in the broader match set so
    // fuzzy/provider-aware lookups can still resolve a valid non-provider key.
    let preferred_with_pricing: Vec<&String> = preferred_matches
        .iter()
        .copied()
        .filter(|k| dataset.get(k.as_str()).is_some_and(has_any_usable_pricing))
        .collect();
    let effective_matches: Vec<&String> =
        if preferred_with_pricing.is_empty() && !provider_matches.is_empty() {
            matches
                .iter()
                .copied()
                .filter(|k| dataset.get(k.as_str()).is_some_and(has_any_usable_pricing))
                .collect()
        } else {
            preferred_with_pricing
        };
    if effective_matches.is_empty() {
        return None;
    }
    let effective_matches = effective_matches.as_slice();

    let hint_is_reseller = provider_id.is_some_and(is_reseller_provider);
    let pick = |candidates: &[&String], prefer_reseller: bool| -> Option<LookupResult> {
        let key = if prefer_reseller {
            candidates
                .iter()
                .find(|k| is_reseller_provider(k))
                .or_else(|| candidates.first())
        } else {
            candidates
                .iter()
                .find(|k| is_original_provider(k))
                .or_else(|| candidates.iter().find(|k| !is_reseller_provider(k)))
                .or_else(|| candidates.first())
        };
        key.and_then(|k| {
            dataset.get(k.as_str()).map(|pricing| LookupResult {
                pricing: pricing.clone(),
                source: source.into(),
                matched_key: (*k).clone(),
            })
        })
    };

    pick(effective_matches, hint_is_reseller)
}

fn model_prefix_matches_provider(model_id: &str, provider_id: Option<&str>) -> bool {
    let Some(hint) = provider_id else {
        return true;
    };
    let Some(prefix) = model_id.split('/').next() else {
        return false;
    };
    let prefix_tag = provider_identity::canonical_provider(prefix);
    let hint_primary = provider_identity::canonical_provider(hint);
    match (prefix_tag, hint_primary) {
        (Some(p), Some(h)) => p == h,
        _ => false,
    }
}

fn parse_provider_scoped_model_path(model_id: &str) -> Option<ProviderScopedModelPath<'_>> {
    let rest = model_id.strip_prefix("accounts/")?;
    let (provider, rest) = rest.split_once('/')?;
    let (scope, terminal_model_id) = rest.split_once('/')?;

    if provider.is_empty() || terminal_model_id.is_empty() {
        return None;
    }

    match scope {
        "models" | "routers" => Some(ProviderScopedModelPath {
            provider,
            terminal_model_id,
        }),
        _ => None,
    }
}

fn provider_hint_matches_scoped_provider(provider_id: Option<&str>, scoped_provider: &str) -> bool {
    let Some(provider_id) = provider_id else {
        return true;
    };

    let scoped_tags = provider_identity::provider_tags(scoped_provider);
    let hint_tags = provider_identity::provider_tags(provider_id);
    !scoped_tags.is_empty()
        && scoped_tags
            .iter()
            .any(|scoped| hint_tags.iter().any(|hint| hint == scoped))
}

fn provider_prefix_matches_scoped_provider(prefix: &str, scoped_tags: &[String]) -> bool {
    if scoped_tags.is_empty() {
        return false;
    }

    provider_identity::provider_tags(prefix.trim_end_matches('/'))
        .iter()
        .any(|prefix_tag| scoped_tags.iter().any(|scoped| scoped == prefix_tag))
}

fn normalize_provider_hint(provider_id: Option<&str>) -> Option<&str> {
    provider_id
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("unknown"))
}

fn build_lookup_cache_key(model_id: &str, provider_id: Option<&str>) -> String {
    match provider_id {
        Some(provider) if !provider.trim().is_empty() => {
            format!("{}|{}", provider.to_lowercase(), model_id.to_lowercase())
        }
        _ => model_id.to_lowercase(),
    }
}

fn model_part_matches_exact(model_part: &str, model_id: &str) -> bool {
    if model_part == model_id {
        return true;
    }

    let mut suffix = model_part;
    while let Some((_, rest)) = suffix.split_once('.') {
        if rest == model_id {
            return true;
        }
        suffix = rest;
    }

    false
}

fn choose_best_source_result(
    litellm_result: Option<LookupResult>,
    openrouter_result: Option<LookupResult>,
    provider_id: Option<&str>,
) -> Option<LookupResult> {
    match (&litellm_result, &openrouter_result) {
        (Some(l), Some(o)) => {
            let l_matches_provider =
                provider_identity::matches_provider_hint(&l.matched_key, provider_id);
            let o_matches_provider =
                provider_identity::matches_provider_hint(&o.matched_key, provider_id);

            if l_matches_provider && !o_matches_provider {
                return litellm_result;
            }
            if o_matches_provider && !l_matches_provider {
                return openrouter_result;
            }

            let l_is_original = is_original_provider(&l.matched_key);
            let o_is_original = is_original_provider(&o.matched_key);
            let l_is_reseller = is_reseller_provider(&l.matched_key);
            let o_is_reseller = is_reseller_provider(&o.matched_key);

            if o_is_original && !l_is_original {
                return openrouter_result;
            }
            if l_is_original && !o_is_original {
                return litellm_result;
            }
            if !l_is_reseller && o_is_reseller {
                return litellm_result;
            }
            if !o_is_reseller && l_is_reseller {
                return openrouter_result;
            }

            litellm_result
        }
        (Some(_), None) => litellm_result,
        (None, Some(_)) => openrouter_result,
        (None, None) => None,
    }
}

fn exact_match_with_provider_prefixes(
    model_id: &str,
    provider_id: Option<&str>,
    key_parts: &[KeyModelPart],
    dataset: &HashMap<String, ModelPricing>,
    source: &str,
) -> Option<LookupResult> {
    let provider_id = provider_id?;
    let hint_tags = provider_identity::provider_tags(provider_id);

    let matches: Vec<&String> = key_parts
        .iter()
        .filter(|kp| {
            model_part_matches_exact(&kp.lower_model_part, model_id)
                && provider_identity::matches_provider_hint_with_tags(&kp.key, &hint_tags)
        })
        .map(|kp| &kp.key)
        .collect();

    if matches.is_empty() {
        return None;
    }

    select_best_match(&matches, dataset, source, Some(provider_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn openai_272k_pricing(input: f64, output: f64) -> ModelPricing {
        ModelPricing {
            input_cost_per_token: Some(input),
            output_cost_per_token: Some(output),
            cache_read_input_token_cost: Some(input / 10.0),
            cache_creation_input_token_cost: Some(input * 1.25),
            input_cost_per_token_above_272k_tokens: Some(input * 2.0),
            output_cost_per_token_above_272k_tokens: Some(output * 1.5),
            cache_read_input_token_cost_above_272k_tokens: Some(input / 5.0),
            ..Default::default()
        }
    }

    #[test]
    fn gpt6_astra_openai_hint_prefers_litellm_standard_over_openrouter_flex() {
        let mut litellm = HashMap::new();
        litellm.insert("gpt-6-astra".to_string(), openai_272k_pricing(1e-5, 5e-5));
        let mut openrouter = HashMap::new();
        openrouter.insert(
            "openai/gpt-6-astra".to_string(),
            openai_272k_pricing(5e-6, 2.5e-5),
        );

        let lookup = PricingLookup::new_with_models_dev(
            litellm,
            openrouter,
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
        );
        let result = lookup
            .lookup_with_provider("gpt-6-astra", Some("openai"))
            .expect("gpt-6-astra should resolve");

        assert_eq!(result.source, "LiteLLM");
        assert_eq!(result.matched_key, "gpt-6-astra");
        assert_eq!(result.pricing.input_cost_per_token, Some(1e-5));
        assert_eq!(result.pricing.output_cost_per_token, Some(5e-5));
    }

    #[test]
    fn gpt6_astra_snapshot_is_openai_full_request_272k_model() {
        assert!(is_openai_full_request_272k_model("gpt-6-astra"));
        assert!(is_openai_full_request_272k_model("openai/gpt-6-astra"));
        assert!(is_openai_full_request_272k_model("gpt-6-astra-20260903"));
        assert!(is_openai_full_request_272k_model("gpt-6-astra-pro"));
        assert!(!is_openai_full_request_272k_model("gpt-5.3"));
    }
}
