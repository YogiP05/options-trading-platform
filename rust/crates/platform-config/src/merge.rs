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
            hint: whitespace_hint(slot, raw),
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

// --- Canonical override grammar ----------------------------------------------
//
// Environment overrides arrive as raw strings, so *how a string becomes a
// value* is as much a part of the shared schema as the value ranges are. These
// grammars are spelled out here and mirrored character for character in
// `py/src/t_plat/config/merge.py`, deliberately **not** delegated to each
// language's standard library: `str::parse` and Python's `int()`/`float()`
// disagree in ways that would silently split the two implementations —
// Python's builtins accept `1_0`, Arabic-Indic and full-width digits, and
// their whitespace stripping differs from Rust's (`str::trim` follows Unicode
// `White_Space`, Python's `str.strip` follows `str.isspace`, and they part
// company on characters like U+001F).
//
// Neither side trims. A value with surrounding whitespace is rejected with an
// error that says so, which is noisier than silently accepting `" 5432"` but
// cannot mean two different things in two languages.

/// Whether `raw` matches the canonical integer grammar: an optional single
/// leading ASCII `+`/`-`, then one or more ASCII digits. No underscores, no
/// whitespace, no radix prefixes, no non-ASCII digits.
///
/// Equivalent to the regex `^[+-]?[0-9]+$`, and to the subset of
/// `i64::from_str` that Python's `int()` can be made to agree with.
fn is_canonical_integer(raw: &str) -> bool {
    let digits = raw.strip_prefix(['+', '-']).unwrap_or(raw);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// Whether `raw` matches the canonical float grammar: an optional sign, then
/// either a non-finite spelling (`inf`, `infinity`, `nan`, case-insensitive,
/// as `f64::from_str` accepts) or a decimal mantissa with at least one ASCII
/// digit and an optional `e`/`E` exponent.
///
/// Equivalent to the regex
/// `^[+-]?(inf|infinity|nan|([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?)$`
/// (case-insensitive). Non-finite spellings parse here and are then rejected
/// by value in [`Config::validate`](crate::Config::validate), identically in
/// both languages.
fn is_canonical_float(raw: &str) -> bool {
    let body = raw.strip_prefix(['+', '-']).unwrap_or(raw);
    if body.is_empty() {
        return false;
    }
    let lowered = body.to_ascii_lowercase();
    if matches!(lowered.as_str(), "inf" | "infinity" | "nan") {
        return true;
    }

    let (mantissa, exponent) = match lowered.split_once('e') {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (lowered.as_str(), None),
    };

    if let Some(exponent) = exponent {
        let exponent_digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if exponent_digits.is_empty() || !exponent_digits.bytes().all(|byte| byte.is_ascii_digit())
        {
            return false;
        }
    }

    let (integral, fractional) = match mantissa.split_once('.') {
        Some((integral, fractional)) => (integral, fractional),
        None => (mantissa, ""),
    };
    // At least one digit overall, and nothing but ASCII digits on either side.
    // A second `.` lands in `fractional` and fails the digit check.
    (!integral.is_empty() || !fractional.is_empty())
        && integral.bytes().all(|byte| byte.is_ascii_digit())
        && fractional.bytes().all(|byte| byte.is_ascii_digit())
}

/// Parse `raw` as the canonical spelling of a boolean.
///
/// ASCII-lowercased rather than [`str::to_lowercase`] so the accepted set
/// cannot widen through Unicode case folding, and untrimmed like the numeric
/// grammars.
fn parse_canonical_bool(raw: &str) -> Option<bool> {
    match raw.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Parse `raw` as the same TOML type `existing` already holds.
///
/// Returns `None` when `raw` does not parse as that type.
fn coerce_like(existing: &Value, raw: &str) -> Option<Value> {
    match existing {
        // Strings are taken verbatim — no trimming, no interpretation — in
        // both languages.
        Value::String(_) => Some(Value::String(raw.to_owned())),
        Value::Integer(_) => {
            // The grammar check rules out everything `i64::from_str` would
            // disagree with Python about; `from_str` then rejects overflow.
            is_canonical_integer(raw)
                .then(|| raw.parse::<i64>().ok())
                .flatten()
                .map(Value::Integer)
        }
        Value::Float(_) => is_canonical_float(raw)
            .then(|| raw.parse::<f64>().ok())
            .flatten()
            .map(Value::Float),
        Value::Boolean(_) => parse_canonical_bool(raw).map(Value::Boolean),
        // Tables and arrays are not overridable one-variable-at-a-time; point
        // the operator at a config file instead of inventing a mini-syntax.
        _ => None,
    }
}

/// Guidance for the common near-miss: a value that would have parsed if it
/// were not padded with whitespace.
///
/// Neither language trims overrides, so this turns a confusing rejection into
/// an obvious one. The Python mirror emits the same sentence.
fn whitespace_hint(existing: &Value, raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed != raw && coerce_like(existing, trimmed).is_some() {
        " — surrounding whitespace is not allowed".to_owned()
    } else {
        String::new()
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
    fn the_integer_grammar_is_ascii_digits_with_an_optional_sign() {
        for accepted in ["0", "10", "+10", "-10", "010", "9223372036854775807"] {
            assert!(
                super::is_canonical_integer(accepted),
                "should accept {accepted:?}"
            );
        }
        for rejected in [
            "1_0", // Python's int() accepts this; Rust's parse does not
            " 10", // no trimming, in either language
            "10 ",
            "10\n",
            "\u{1f}10", // Python's str.strip removes this; Rust's trim does not
            "\u{a0}10", // Rust's trim removes this; the grammar does not
            "0x10",
            "1 0",
            "1,0",
            "10.0",
            "1e1",
            "\u{661}\u{660}",   // Arabic-Indic digits
            "\u{ff11}\u{ff10}", // full-width digits
            "",
            "+",
            "-",
            "++1",
        ] {
            assert!(
                !super::is_canonical_integer(rejected),
                "should reject {rejected:?}"
            );
        }
    }

    #[test]
    fn the_float_grammar_matches_rusts_spellings_without_the_lenient_extras() {
        for accepted in [
            "0", "0.5", "+0.5", "-0.5", ".5", "0.", "1e3", "1E3", "1e+3", "1e-3", "5e-1", "inf",
            "INF", "infinity", "nan", "-inf",
        ] {
            assert!(
                super::is_canonical_float(accepted),
                "should accept {accepted:?}"
            );
        }
        for rejected in [
            "0_.5",
            "0.5_0", // Python's float() accepts this; Rust's parse does not
            " 0.5",
            "0.5 ",
            "\u{1f}0.5",
            ".",
            "",
            "+",
            "1e",
            "e3",
            "0x1p3",
            "1.2.3",
            "1e2e3",
            "\u{660}.\u{665}", // Arabic-Indic digits
        ] {
            assert!(
                !super::is_canonical_float(rejected),
                "should reject {rejected:?}"
            );
        }
    }

    #[test]
    fn the_boolean_grammar_is_untrimmed_and_ascii_cased() {
        assert_eq!(super::parse_canonical_bool("TRUE"), Some(true));
        assert_eq!(super::parse_canonical_bool("off"), Some(false));
        assert_eq!(super::parse_canonical_bool(" true"), None, "no trimming");
        assert_eq!(super::parse_canonical_bool("true "), None, "no trimming");
        assert_eq!(super::parse_canonical_bool(""), None);
    }

    #[test]
    fn a_whitespace_near_miss_says_so() {
        let mut base = doc("[db]\nport = 5432\n");
        let err = apply_env_overrides(&mut base, &env(&[("T_PLAT__DB__PORT", " 6543 ")]))
            .expect_err("untrimmed value must fail");
        assert!(
            format!("{err}").contains("surrounding whitespace"),
            "got: {err}"
        );
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
