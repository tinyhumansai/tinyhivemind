//! Side-effect-free canonical parsing of the internal deployment plan.
#[cfg(test)]
use super::HiveConfig;
use super::{ConfigError, Result};
use serde_json::Value;

#[cfg(test)]
impl HiveConfig {
    /// Parse canonical JSON without resolving credentials or opening files.
    ///
    /// # Errors
    /// Returns `Json` for malformed/unknown fields and `InlineSecret` for literals.
    pub(crate) fn from_json(input: &str) -> Result<Self> {
        let value = serde_json::from_str(input).map_err(|_| ConfigError::Json)?;
        Self::from_value(value)
    }
    /// Parse a canonical value without runtime construction.
    ///
    /// # Errors
    /// Returns typed structure or inline credential failures.
    pub(crate) fn from_value(value: Value) -> Result<Self> {
        reject_inline(&value, "config")?;
        let original = value.clone();
        let config: Self = serde_json::from_value(value).map_err(|_| ConfigError::Json)?;
        let canonical = serde_json::to_value(&config).map_err(|_| ConfigError::Json)?;
        reject_unknown(&original, &canonical)?;
        Ok(config)
    }
}

pub(super) fn reject_inline(value: &Value, field: &str) -> Result<()> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let next = format!("{field}.{key}");
                if key == "endpoint" && child.as_str().is_some_and(endpoint_has_secret) {
                    return Err(ConfigError::InlineSecret { field: next });
                }
                if matches!(
                    key.as_str(),
                    "credential" | "api_key" | "password" | "token" | "secret"
                ) && !child.is_null()
                {
                    let safe = child.as_object().is_some_and(|m| {
                        m.len() == 1
                            && m.iter().all(|(k, v)| {
                                matches!(k.as_str(), "env" | "store")
                                    && v.as_str().is_some_and(|s| !s.trim().is_empty())
                            })
                    });
                    if !safe {
                        return Err(ConfigError::InlineSecret { field: next });
                    }
                }
                if matches!(key.as_str(), "env" | "headers") && child.is_object() {
                    for item in child.as_object().into_iter().flat_map(|m| m.values()) {
                        let safe = item.as_object().is_some_and(|m| {
                            m.len() == 1
                                && m.iter().all(|(k, v)| {
                                    matches!(k.as_str(), "env" | "store")
                                        && v.as_str().is_some_and(|s| !s.trim().is_empty())
                                })
                        });
                        if !safe {
                            return Err(ConfigError::InlineSecret { field: next });
                        }
                    }
                } else {
                    reject_inline(child, &next)?;
                }
            }
        }
        Value::Array(items) => {
            for child in items {
                reject_inline(child, field)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn endpoint_has_secret(endpoint: &str) -> bool {
    let authority = endpoint
        .split_once("://")
        .map(|(_, tail)| tail.split('/').next().unwrap_or(tail));
    authority.is_some_and(|a| a.contains('@'))
        || endpoint.split_once('?').is_some_and(|(_, query)| {
            query.split('&').any(|item| {
                item.split_once('=').is_some_and(|(key, _)| {
                    matches!(
                        key.to_ascii_lowercase().as_str(),
                        "token" | "api_key" | "apikey" | "password" | "secret" | "access_token"
                    )
                })
            })
        })
}

// Existing native policies intentionally accept unknown fields outside manifests.
// Compare the raw structure to its fully typed wire form at this boundary.
#[cfg(test)]
fn reject_unknown(raw: &Value, canonical: &Value) -> Result<()> {
    match (raw, canonical) {
        (Value::Object(raw), Value::Object(typed)) => {
            for (key, value) in raw {
                let decoded = typed.get(key).ok_or(ConfigError::Json)?;
                // Native ToolRules already denies unknown fields, but omits
                // optional nulls and empty predicates from its serialized form.
                if key == "tool_rules" {
                    continue;
                }
                reject_unknown(value, decoded)?;
            }
        }
        (Value::Array(raw), Value::Array(typed)) => {
            for (value, decoded) in raw.iter().zip(typed) {
                reject_unknown(value, decoded)?;
            }
        }
        _ => {}
    }
    Ok(())
}
