use super::litellm::ModelPricing;
use crate::sessions::synthetic::normalize_synthetic_model;
use serde::de::{MapAccess, Visitor};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

const CUSTOM_PRICING_FILENAME: &str = "custom-pricing.json";
const TOKENS_PER_MILLION: f64 = 1_000_000.0;
const MAX_CUSTOM_PRICING_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_CUSTOM_PRICING_MODEL_CAPACITY: usize = 10_000;

#[derive(Clone, Default)]
pub struct CustomPricing {
    models: HashMap<String, ModelPricing>,
}

pub struct CustomLookupResult<'a> {
    pub matched_key: &'a str,
    pub pricing: &'a ModelPricing,
}

#[derive(Deserialize)]
struct RawCustomPricingFile {
    #[serde(default, deserialize_with = "deserialize_models")]
    models: Vec<(String, Value)>,
}

#[derive(Deserialize)]
struct CustomModelPricing {
    input_cost_per_million_tokens: Option<f64>,
    input_cost_per_million_tokens_above_128k_tokens: Option<f64>,
    input_cost_per_million_tokens_above_200k_tokens: Option<f64>,
    input_cost_per_million_tokens_above_256k_tokens: Option<f64>,
    input_cost_per_million_tokens_above_272k_tokens: Option<f64>,
    input_cost_per_token: Option<f64>,
    input_cost_per_token_above_128k_tokens: Option<f64>,
    input_cost_per_token_above_200k_tokens: Option<f64>,
    input_cost_per_token_above_256k_tokens: Option<f64>,
    input_cost_per_token_above_272k_tokens: Option<f64>,
    output_cost_per_million_tokens: Option<f64>,
    output_cost_per_million_tokens_above_128k_tokens: Option<f64>,
    output_cost_per_million_tokens_above_200k_tokens: Option<f64>,
    output_cost_per_million_tokens_above_256k_tokens: Option<f64>,
    output_cost_per_million_tokens_above_272k_tokens: Option<f64>,
    output_cost_per_token: Option<f64>,
    output_cost_per_token_above_128k_tokens: Option<f64>,
    output_cost_per_token_above_200k_tokens: Option<f64>,
    output_cost_per_token_above_256k_tokens: Option<f64>,
    output_cost_per_token_above_272k_tokens: Option<f64>,
    cache_creation_input_token_cost_per_million_tokens: Option<f64>,
    cache_creation_input_token_cost_per_million_tokens_above_200k_tokens: Option<f64>,
    cache_creation_input_token_cost: Option<f64>,
    cache_creation_input_token_cost_above_200k_tokens: Option<f64>,
    cache_read_input_token_cost_per_million_tokens: Option<f64>,
    cache_read_input_token_cost_per_million_tokens_above_200k_tokens: Option<f64>,
    cache_read_input_token_cost_per_million_tokens_above_272k_tokens: Option<f64>,
    cache_read_input_token_cost: Option<f64>,
    cache_read_input_token_cost_above_200k_tokens: Option<f64>,
    cache_read_input_token_cost_above_272k_tokens: Option<f64>,
}

impl CustomModelPricing {
    fn into_model_pricing(self) -> Result<ModelPricing, String> {
        let input_cost_per_token = base_price(
            self.input_cost_per_million_tokens,
            self.input_cost_per_token,
            "input_cost_per_million_tokens",
            "input_cost_per_token",
        )?;
        let output_cost_per_token = base_price(
            self.output_cost_per_million_tokens,
            self.output_cost_per_token,
            "output_cost_per_million_tokens",
            "output_cost_per_token",
        )?;

        if !input_cost_per_token.is_some_and(|value| value > 0.0)
            && !output_cost_per_token.is_some_and(|value| value > 0.0)
        {
            return Err(
                "at least one of input or output pricing must be present and positive".into(),
            );
        }

        Ok(ModelPricing {
            input_cost_per_token,
            input_cost_per_token_above_128k_tokens: price_field(
                self.input_cost_per_million_tokens_above_128k_tokens,
                self.input_cost_per_token_above_128k_tokens,
                "input_cost_per_million_tokens_above_128k_tokens",
                "input_cost_per_token_above_128k_tokens",
            )?,
            input_cost_per_token_above_200k_tokens: price_field(
                self.input_cost_per_million_tokens_above_200k_tokens,
                self.input_cost_per_token_above_200k_tokens,
                "input_cost_per_million_tokens_above_200k_tokens",
                "input_cost_per_token_above_200k_tokens",
            )?,
            input_cost_per_token_above_256k_tokens: price_field(
                self.input_cost_per_million_tokens_above_256k_tokens,
                self.input_cost_per_token_above_256k_tokens,
                "input_cost_per_million_tokens_above_256k_tokens",
                "input_cost_per_token_above_256k_tokens",
            )?,
            input_cost_per_token_above_272k_tokens: price_field(
                self.input_cost_per_million_tokens_above_272k_tokens,
                self.input_cost_per_token_above_272k_tokens,
                "input_cost_per_million_tokens_above_272k_tokens",
                "input_cost_per_token_above_272k_tokens",
            )?,
            output_cost_per_token,
            output_cost_per_token_above_128k_tokens: price_field(
                self.output_cost_per_million_tokens_above_128k_tokens,
                self.output_cost_per_token_above_128k_tokens,
                "output_cost_per_million_tokens_above_128k_tokens",
                "output_cost_per_token_above_128k_tokens",
            )?,
            output_cost_per_token_above_200k_tokens: price_field(
                self.output_cost_per_million_tokens_above_200k_tokens,
                self.output_cost_per_token_above_200k_tokens,
                "output_cost_per_million_tokens_above_200k_tokens",
                "output_cost_per_token_above_200k_tokens",
            )?,
            output_cost_per_token_above_256k_tokens: price_field(
                self.output_cost_per_million_tokens_above_256k_tokens,
                self.output_cost_per_token_above_256k_tokens,
                "output_cost_per_million_tokens_above_256k_tokens",
                "output_cost_per_token_above_256k_tokens",
            )?,
            output_cost_per_token_above_272k_tokens: price_field(
                self.output_cost_per_million_tokens_above_272k_tokens,
                self.output_cost_per_token_above_272k_tokens,
                "output_cost_per_million_tokens_above_272k_tokens",
                "output_cost_per_token_above_272k_tokens",
            )?,
            cache_creation_input_token_cost: price_field(
                self.cache_creation_input_token_cost_per_million_tokens,
                self.cache_creation_input_token_cost,
                "cache_creation_input_token_cost_per_million_tokens",
                "cache_creation_input_token_cost",
            )?,
            cache_creation_input_token_cost_above_200k_tokens: price_field(
                self.cache_creation_input_token_cost_per_million_tokens_above_200k_tokens,
                self.cache_creation_input_token_cost_above_200k_tokens,
                "cache_creation_input_token_cost_per_million_tokens_above_200k_tokens",
                "cache_creation_input_token_cost_above_200k_tokens",
            )?,
            cache_read_input_token_cost: price_field(
                self.cache_read_input_token_cost_per_million_tokens,
                self.cache_read_input_token_cost,
                "cache_read_input_token_cost_per_million_tokens",
                "cache_read_input_token_cost",
            )?,
            cache_read_input_token_cost_above_200k_tokens: price_field(
                self.cache_read_input_token_cost_per_million_tokens_above_200k_tokens,
                self.cache_read_input_token_cost_above_200k_tokens,
                "cache_read_input_token_cost_per_million_tokens_above_200k_tokens",
                "cache_read_input_token_cost_above_200k_tokens",
            )?,
            cache_read_input_token_cost_above_272k_tokens: price_field(
                self.cache_read_input_token_cost_per_million_tokens_above_272k_tokens,
                self.cache_read_input_token_cost_above_272k_tokens,
                "cache_read_input_token_cost_per_million_tokens_above_272k_tokens",
                "cache_read_input_token_cost_above_272k_tokens",
            )?,
        })
    }
}

impl CustomPricing {
    pub fn default_path() -> PathBuf {
        crate::paths::get_config_dir().join(CUSTOM_PRICING_FILENAME)
    }

    pub fn load_from_default_path() -> Self {
        Self::load_from_path(&Self::default_path())
    }

    pub fn load_from_path(path: &Path) -> Self {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(err) => {
                warn_custom_pricing(path, format_args!("failed to stat file: {err}"));
                return Self::default();
            }
        };

        if metadata.len() > MAX_CUSTOM_PRICING_FILE_BYTES {
            warn_custom_pricing(
                path,
                format_args!(
                    "file is too large ({} bytes; max {} bytes)",
                    metadata.len(),
                    MAX_CUSTOM_PRICING_FILE_BYTES
                ),
            );
            return Self::default();
        }

        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(err) => {
                warn_custom_pricing(path, format_args!("failed to read file: {err}"));
                return Self::default();
            }
        };

        Self::load_from_str(&content, path)
    }

    pub fn from_models(models: HashMap<String, ModelPricing>) -> Self {
        let mut normalized =
            HashMap::with_capacity(models.len().min(MAX_CUSTOM_PRICING_MODEL_CAPACITY));
        for (key, pricing) in models {
            normalized.insert(key.to_lowercase(), pricing);
        }
        Self { models: normalized }
    }

    pub fn len(&self) -> usize {
        self.models.len()
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    pub fn entries(&self) -> impl Iterator<Item = (&str, &ModelPricing)> {
        self.models
            .iter()
            .map(|(model_id, pricing)| (model_id.as_str(), pricing))
    }

    pub fn lookup(&self, model_id: &str) -> Option<&ModelPricing> {
        self.lookup_with_key(model_id).map(|result| result.pricing)
    }

    pub fn lookup_with_key(&self, model_id: &str) -> Option<CustomLookupResult<'_>> {
        let raw_key = model_id.to_lowercase();
        if let Some(pricing) = self.models.get_key_value(&raw_key) {
            return Some(CustomLookupResult {
                matched_key: pricing.0,
                pricing: pricing.1,
            });
        }

        let normalized_key = normalize_synthetic_model(model_id).to_lowercase();
        if normalized_key != raw_key {
            if let Some(pricing) = self.models.get_key_value(&normalized_key) {
                return Some(CustomLookupResult {
                    matched_key: pricing.0,
                    pricing: pricing.1,
                });
            }
        }

        None
    }

    fn load_from_str(content: &str, path: &Path) -> Self {
        let raw: RawCustomPricingFile = match serde_json::from_str(content) {
            Ok(raw) => raw,
            Err(err) => {
                warn_custom_pricing(path, format_args!("failed to parse JSON: {err}"));
                return Self::default();
            }
        };

        let mut models =
            HashMap::with_capacity(raw.models.len().min(MAX_CUSTOM_PRICING_MODEL_CAPACITY));
        for (model_id, value) in raw.models {
            let lower_key = model_id.to_lowercase();
            let entry: CustomModelPricing = match serde_json::from_value(value) {
                Ok(entry) => entry,
                Err(err) => {
                    warn_custom_pricing(
                        path,
                        format_args!("skipping {model_id}: malformed pricing entry: {err}"),
                    );
                    continue;
                }
            };
            let pricing = match entry.into_model_pricing() {
                Ok(pricing) => pricing,
                Err(err) => {
                    warn_custom_pricing(path, format_args!("skipping {model_id}: {err}"));
                    continue;
                }
            };

            if models.insert(lower_key.clone(), pricing).is_some() {
                warn_custom_pricing(
                    path,
                    format_args!(
                        "duplicate model key after lowercasing, last entry wins: {lower_key}"
                    ),
                );
            }
        }

        Self { models }
    }
}

fn base_price(
    per_million: Option<f64>,
    per_token: Option<f64>,
    per_million_field: &str,
    per_token_field: &str,
) -> Result<Option<f64>, String> {
    price_field(per_million, per_token, per_million_field, per_token_field)
}

fn price_field(
    per_million: Option<f64>,
    per_token: Option<f64>,
    per_million_field: &str,
    per_token_field: &str,
) -> Result<Option<f64>, String> {
    match (per_million, per_token) {
        (Some(_), Some(_)) => Err(format!(
            "{per_million_field} and {per_token_field} cannot both be set"
        )),
        (Some(value), None) => validate_non_negative(value, per_million_field).map(to_per_token),
        (None, Some(value)) => validate_non_negative(value, per_token_field),
        (None, None) => Ok(None),
    }
}

fn validate_non_negative(value: f64, field: &str) -> Result<Option<f64>, String> {
    if value.is_finite() && value >= 0.0 {
        Ok(Some(value))
    } else {
        Err(format!("{field} must be non-negative and finite"))
    }
}

fn to_per_token(per_million: Option<f64>) -> Option<f64> {
    let per_million = per_million?;
    Some(per_million / TOKENS_PER_MILLION)
}

fn warn_custom_pricing(path: &Path, message: fmt::Arguments<'_>) {
    eprintln!(
        "[tokens] Warning: custom pricing {}: {message}",
        path.display()
    );
}

fn deserialize_models<'de, D>(deserializer: D) -> Result<Vec<(String, Value)>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct ModelsVisitor;

    impl<'de> Visitor<'de> for ModelsVisitor {
        type Value = Vec<(String, Value)>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a map of model ids to pricing entries")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut entries = Vec::with_capacity(
                access
                    .size_hint()
                    .unwrap_or(0)
                    .min(MAX_CUSTOM_PRICING_MODEL_CAPACITY),
            );
            while let Some((key, value)) = access.next_entry::<String, Value>()? {
                entries.push((key, value));
            }
            Ok(entries)
        }
    }

    deserializer.deserialize_map(ModelsVisitor)
}

