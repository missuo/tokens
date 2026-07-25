//! Config-driven model-name aliasing for grouping.
//!
//! Different supply channels report the same physical model under different
//! name-strings (for example `claude-opus-4-8`, `claude-opus-4-8-cc`, and
//! `anthropic/claude-opus-4-8` are all one model), so usage stats split across
//! multiple rows. A user-configured `{alias: canonical}` map, read from
//! `settings.json` under `modelAliases`, folds those variants into one canonical
//! model.
//!
//! The fold runs as the terminal step of [`crate::normalize_model_for_grouping`],
//! so it applies uniformly to the local models report, every `--group-by`,
//! monthly and hourly reports, and the TUI. It is **presentation only**: the
//! submit/upload/export/persist path uses [`crate::canonical_model_id`] (the
//! same syntactic normalization *without* the alias fold), so a machine-local
//! alias config can never rewrite the model identity that leaves the machine or
//! fragment history across a user's devices. It is deliberately **not** applied
//! before pricing (per-message cost is computed on the raw model id upstream), so
//! folding can only relabel and merge already-costed buckets and can never change
//! a cost total. It is orthogonal to the pricing alias table
//! ([`crate::pricing`]) and to `provider_identity` — it touches only the model
//! dimension.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

/// Upper bound on the number of configured aliases retained. Oversized configs
/// are truncated rather than rejected, mirroring the capacity guard in
/// [`crate::pricing`]'s custom-pricing loader.
const MAX_MODEL_ALIASES: usize = 4096;

/// On-disk shape of the flat `modelAliases` object in `settings.json`
/// (`{ "alias": "canonical" }`). `#[serde(transparent)]` keeps the serialized
/// form a bare map. Deserialization is lossy: a malformed value (a non-object,
/// or an entry whose value is not a string) is skipped instead of failing the
/// whole settings load.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ModelAliasMap {
    /// Raw `alias -> canonical` pairs exactly as written in the config.
    pub entries: BTreeMap<String, String>,
}

impl<'de> Deserialize<'de> for ModelAliasMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Read the node as a generic value first so a malformed `modelAliases`
        // (e.g. an array or scalar) degrades to an empty map instead of
        // misaligning the parent settings deserializer. Keep only string-valued
        // entries; skip anything else.
        let value = serde_json::Value::deserialize(deserializer)?;
        let entries = match value {
            serde_json::Value::Object(object) => object
                .into_iter()
                .filter_map(|(key, value)| match value {
                    serde_json::Value::String(canonical) => Some((key, canonical)),
                    _ => None,
                })
                .collect(),
            _ => BTreeMap::new(),
        };
        Ok(Self { entries })
    }
}

/// Runtime resolver built from [`ModelAliasMap`]: keys and values are normalized
/// through [`crate::normalize_syntactic`] so lookups match regardless of case,
/// dated suffix, or `.`-vs-`-` spelling, and canonical values land in the same
/// space the grouping key uses. Empty keys/values and self-maps are dropped; the
/// number of entries is capped.
#[derive(Debug, Default)]
pub(crate) struct ModelAliasResolver {
    map: HashMap<String, String>,
}

impl ModelAliasResolver {
    /// Build a resolver from configured aliases. Both sides of each pair are run
    /// through [`crate::normalize_syntactic`] exactly once: keys are placed in the
    /// same space as incoming (already-normalized) model names, and canonical
    /// values are stored pre-normalized. `apply` returns a canonical value
    /// verbatim — it is never re-resolved or re-normalized — so the value written
    /// here is exactly the label shown in reports. Empty keys/values and
    /// self-maps are dropped, and the number of entries is capped.
    pub(crate) fn from_config(config: &ModelAliasMap) -> Self {
        let mut map = HashMap::new();
        for (raw_alias, raw_canonical) in &config.entries {
            if map.len() >= MAX_MODEL_ALIASES {
                break;
            }
            // Store keys under a separator-insensitive match key so matching is
            // provider-agnostic (not claude-only): `gpt-5-5` and `gpt-5.5` share
            // the key `gpt-5-5`. The stored canonical value keeps its
            // `normalize_syntactic` spelling — it is the label shown verbatim.
            let alias_norm = crate::normalize_syntactic(raw_alias);
            let canonical = crate::normalize_syntactic(raw_canonical);
            // Self-map drop compares the *exact* normalized forms, not the match
            // keys: `{gpt-5-5: gpt-5.5}` is a real separator relabel that must be
            // kept, whereas `{gpt-5.5: gpt-5.5}` is a genuine no-op to drop.
            if alias_norm.is_empty() || canonical.is_empty() || alias_norm == canonical {
                continue;
            }
            map.insert(match_key(&alias_norm), canonical);
        }
        Self { map }
    }

    /// Resolve one model name. `name` must already be `normalize_syntactic`'d (it
    /// is, since the only caller is [`crate::normalize_model_for_grouping`]).
    /// Resolution is single-hop — the canonical value is never re-resolved — so
    /// alias chains collapse one step and cycles are structurally impossible.
    /// Returns `name` unchanged on a miss.
    pub(crate) fn apply(&self, name: String) -> String {
        match self.map.get(&match_key(&name)) {
            Some(canonical) => canonical.clone(),
            None => name,
        }
    }
}

/// Reduce an already-`normalize_syntactic`'d model name to a separator-
/// insensitive match key by rewriting every `.` to `-`. This generalizes alias
/// matching beyond claude: `normalize_syntactic` only rewrites `.`→`-` inside
/// *claude* version numbers, so without this a `gpt-5-5` alias would miss
/// `gpt-5.5`. Folding on the match key alone keeps the displayed canonical form
/// (e.g. `gpt-5.5`) untouched for models that were never aliased.
fn match_key(normalized: &str) -> String {
    normalized.replace('.', "-")
}

static GLOBAL: OnceLock<ModelAliasResolver> = OnceLock::new();
static EMPTY: OnceLock<ModelAliasResolver> = OnceLock::new();

/// Install the process-wide model-alias resolver. Intended to be called once at
/// startup (mirroring the pricing service's one-time init); the first call wins
/// and later calls are ignored. Until this is called, grouping is a strict
/// identity no-op, so callers and tests that never install a resolver are
/// unaffected.
pub fn set_global(config: &ModelAliasMap) {
    let _ = GLOBAL.set(ModelAliasResolver::from_config(config));
}

/// The installed resolver, or a shared empty one (identity no-op) when unset.
pub(crate) fn global() -> &'static ModelAliasResolver {
    GLOBAL
        .get()
        .unwrap_or_else(|| EMPTY.get_or_init(ModelAliasResolver::default))
}

