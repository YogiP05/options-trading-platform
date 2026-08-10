//! Layer merging and typed environment overrides.
//!
//! Config layers are merged as TOML documents before they are deserialized
//! into the schema, which is what lets a higher layer state only the keys it
//! changes.

use std::collections::BTreeMap;

use toml::Value;

use crate::error::ConfigError;

/// Prefix marking an environment variable as a config value override.
pub(crate) const ENV_VALUE_PREFIX: &str = "T_PLAT__";

/// Separator between nesting levels inside an override variable name.
pub(crate) const ENV_PATH_SEPARATOR: &str = "__";

/// Deep-merge `overlay` on top of `base`.
///
/// Tables merge key by key; every other value (scalars, arrays) is replaced
/// wholesale. Arrays are deliberately not concatenated — "the later layer
/// wins" is easier to reason about than accumulation you cannot undo.
pub(crate) fn merge_into(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Table(base_table), Value::Table(overlay_table)) => {
            for (key, overlay_value) in overlay_table {
                match base_table.get_mut(&key) {
                    Some(base_value) => merge_into(base_value, overlay_value),
                    None => {
                        base_table.insert(key, overlay_value);
                    }
                }
            }
        }
        (base_slot, overlay_value) => *base_slot = overlay_value,
    }
}

/// Apply `T_PLAT__SECTION__KEY` overrides to an already-merged document.
///
/// The value is parsed as the type the document already holds at that path
/// (which always exists, because layer 1 is the full built-in default set), so
/// overrides stay typed instead of degrading everything to strings.
///
/// Returns the dotted keys that were overridden, sorted, for diagnostics.
///
/// # Errors
///
/// [`ConfigError::UnknownEnvKey`] if the variable targets a key the schema
/// does not define, or [`ConfigError::InvalidEnvValue`] if the value does not
/// parse as the declared type.
pub(crate) fn apply_env_overrides(
    document: &mut Value,
    env: &BTreeMap<String, String>,
) -> Result<Vec<String>, ConfigError> {
    let mut applied = Vec::new();

    for (var, raw) in env {
        let Some(suffix) = var.strip_prefix(ENV_VALUE_PREFIX) else {
            continue;
        };
        let segments: Vec<String> = suffix
            .split(ENV_PATH_SEPARATOR)
            .map(str::to_ascii_lowercase)
            .collect();
        let key = segments.join(".");

        if segments.iter().any(String::is_empty) {
            return Err(ConfigError::UnknownEnvKey {
                var: var.clone(),
                key,
            });
        }

        let slot = resolve_slot(document, &segments).ok_or_else(|| ConfigError::UnknownEnvKey {
            var: var.clone(),
            key: key.clone(),
        })?;

        *slot = coerce_like(slot, raw).ok_or_else(|| ConfigError::InvalidEnvValue {
            var: var.clone(),
            key: key.clone(),
            expected: type_name(slot),
            value: raw.clone(),
        })?;

        applied.push(key);
    }

    applied.sort();
    Ok(applied)
}

/// Walk `document` to the value at `segments`, or `None` if the path does not
/// exist (or runs through a non-table).
fn resolve_slot<'a>(document: &'a mut Value, segments: &[String]) -> Option<&'a mut Value> {
    let mut cursor = document;
    for segment in segments {
        cursor = cursor.as_table_mut()?.get_mut(segment)?;
    }
    Some(cursor)
}

/// Parse `raw` as the same TOML type `existing` already holds.
///
/// Returns `None` when `raw` does not parse as that type.
fn coerce_like(existing: &Value, raw: &str) -> Option<Value> {
    match existing {
        Value::String(_) => Some(Value::String(raw.to_owned())),
        Value::Integer(_) => raw.trim().parse::<i64>().ok().map(Value::Integer),
        Value::Float(_) => raw.trim().parse::<f64>().ok().map(Value::Float),
        Value::Boolean(_) => match raw.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(Value::Boolean(true)),
            "false" | "0" | "no" | "off" => Some(Value::Boolean(false)),
            _ => None,
        },
        // Tables and arrays are not overridable one-variable-at-a-time; point
        // the operator at a config file instead of inventing a mini-syntax.
        _ => None,
    }
}

/// The schema type name used in [`ConfigError::InvalidEnvValue`].
fn type_name(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Integer(_) => "integer",
        Value::Float(_) => "float",
        Value::Boolean(_) => "boolean",
        Value::Datetime(_) => "datetime",
        Value::Array(_) => "array",
        Value::Table(_) => "table",
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_env_overrides, merge_into};
    use std::collections::BTreeMap;
    use toml::Value;

    fn doc(text: &str) -> Value {
        toml::from_str::<Value>(text).expect("valid TOML")
    }

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn tables_merge_and_scalars_replace() {
        let mut base = doc("[a]\nx = 1\ny = 2\n\n[b]\nz = 3\n");
        merge_into(&mut base, doc("[a]\ny = 20\n\n[c]\nw = 4\n"));

        assert_eq!(
            base["a"]["x"].as_integer(),
            Some(1),
            "untouched key survives"
        );
        assert_eq!(base["a"]["y"].as_integer(), Some(20), "overlay wins");
        assert_eq!(
            base["b"]["z"].as_integer(),
            Some(3),
            "untouched table survives"
        );
        assert_eq!(base["c"]["w"].as_integer(), Some(4), "new table is added");
    }

    #[test]
    fn arrays_are_replaced_not_concatenated() {
        let mut base = doc("[a]\nxs = [1, 2, 3]\n");
        merge_into(&mut base, doc("[a]\nxs = [9]\n"));
        assert_eq!(base["a"]["xs"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn overrides_are_parsed_as_the_declared_type() {
        let mut base = doc("[db]\nport = 5432\nhost = \"h\"\n\n[t]\non = false\nrate = 1.0\n");
        let applied = apply_env_overrides(
            &mut base,
            &env(&[
                ("T_PLAT__DB__PORT", "6543"),
                ("T_PLAT__DB__HOST", "elsewhere"),
                ("T_PLAT__T__ON", "true"),
                ("T_PLAT__T__RATE", "0.25"),
                ("PATH", "/usr/bin"),
            ]),
        )
        .expect("overrides apply");

        assert_eq!(base["db"]["port"].as_integer(), Some(6543));
        assert_eq!(base["db"]["host"].as_str(), Some("elsewhere"));
        assert_eq!(base["t"]["on"].as_bool(), Some(true));
        assert_eq!(base["t"]["rate"].as_float(), Some(0.25));
        assert_eq!(applied, ["db.host", "db.port", "t.on", "t.rate"]);
    }

    #[test]
    fn single_underscores_stay_inside_a_segment() {
        let mut base = doc("[market_data]\ntimeout_ms = 1\n");
        apply_env_overrides(
            &mut base,
            &env(&[("T_PLAT__MARKET_DATA__TIMEOUT_MS", "42")]),
        )
        .expect("override applies");
        assert_eq!(base["market_data"]["timeout_ms"].as_integer(), Some(42));
    }

    #[test]
    fn a_typo_in_the_variable_name_is_an_error() {
        let mut base = doc("[db]\nport = 5432\n");
        let err = apply_env_overrides(&mut base, &env(&[("T_PLAT__DBB__PORT", "1")]))
            .expect_err("unknown key must fail");
        assert!(format!("{err}").contains("dbb.port"), "got: {err}");
    }

    #[test]
    fn a_bad_value_is_an_error_not_a_silent_zero() {
        let mut base = doc("[db]\nport = 5432\n");
        let err = apply_env_overrides(&mut base, &env(&[("T_PLAT__DB__PORT", "nope")]))
            .expect_err("bad value must fail");
        let message = format!("{err}");
        assert!(message.contains("integer"), "got: {message}");
        assert_eq!(
            base["db"]["port"].as_integer(),
            Some(5432),
            "left unchanged"
        );
    }
}
