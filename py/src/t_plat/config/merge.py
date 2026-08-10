"""Layer merging and typed environment overrides.

Config layers are merged as plain TOML documents before they are parsed into
the schema, which is what lets a higher layer state only the keys it changes.

Mirrors ``rust/crates/platform-config/src/merge.rs``.
"""

from __future__ import annotations

import re
from collections.abc import Mapping
from typing import Any

from t_plat.config.errors import EnvOverrideError

__all__ = ["ENV_PATH_SEPARATOR", "ENV_VALUE_PREFIX", "apply_env_overrides", "deep_merge"]

#: Prefix marking an environment variable as a config value override.
ENV_VALUE_PREFIX = "T_PLAT__"

#: Separator between nesting levels inside an override variable name.
ENV_PATH_SEPARATOR = "__"

_TRUTHY = frozenset({"true", "1", "yes", "on"})
_FALSY = frozenset({"false", "0", "no", "off"})

# --- Canonical override grammar ----------------------------------------------
#
# Environment overrides arrive as raw strings, so *how a string becomes a value*
# is as much a part of the shared schema as the value ranges are. These grammars
# are spelled out here and mirrored character for character in
# ``rust/crates/platform-config/src/merge.rs``, deliberately **not** delegated to
# ``int()``/``float()``, whose leniency does not match Rust's ``str::parse``:
# Python's builtins accept ``1_0``, Arabic-Indic and full-width digits, and their
# whitespace stripping differs from Rust's (``str::trim`` follows Unicode
# ``White_Space``, ``str.strip`` follows ``str.isspace``, and they part company on
# characters like U+001F).
#
# Neither side trims. A value with surrounding whitespace is rejected with an
# error that says so, which is noisier than silently accepting ``" 5432"`` but
# cannot mean two different things in two languages.
#
# ``[0-9]`` rather than ``\d``: ``\d`` matches Unicode digits, which Rust rejects.

#: Optional single leading ASCII sign, then one or more ASCII digits.
_INTEGER_RE = re.compile(r"[+-]?[0-9]+")

#: Optional sign, then a non-finite spelling (as ``f64::from_str`` accepts) or a
#: decimal mantissa with at least one ASCII digit and an optional exponent.
_FLOAT_RE = re.compile(
    r"[+-]?(?:inf|infinity|nan|(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?)",
    re.IGNORECASE,
)

# TOML integers are signed 64-bit, and the Rust loader parses an integer
# override with `str::parse::<i64>()`. Python integers are unbounded, so the
# same domain is enforced here — otherwise an override too large for TOML would
# be accepted in Python and rejected in Rust.
_TOML_INT_MIN = -(2**63)
_TOML_INT_MAX = 2**63 - 1


def deep_merge(base: dict[str, Any], overlay: Mapping[str, Any]) -> dict[str, Any]:
    """Return ``base`` with ``overlay`` merged on top.

    Tables merge key by key; every other value (scalars, arrays) is replaced
    wholesale. Lists are deliberately not concatenated — "the later layer wins"
    is easier to reason about than accumulation you cannot undo.

    ``base`` is not mutated.
    """
    merged = dict(base)
    for key, overlay_value in overlay.items():
        base_value = merged.get(key)
        if isinstance(base_value, dict) and isinstance(overlay_value, Mapping):
            merged[key] = deep_merge(base_value, overlay_value)
        else:
            merged[key] = overlay_value
    return merged


def apply_env_overrides(document: dict[str, Any], env: Mapping[str, str]) -> list[str]:
    """Apply ``T_PLAT__SECTION__KEY`` overrides to an already-merged document.

    The value is parsed as the type the document already holds at that path
    (which always exists, because layer 1 is the full built-in default set), so
    overrides stay typed instead of degrading everything to strings.

    ``document`` is mutated in place. Returns the dotted keys that were
    overridden, sorted, for diagnostics.

    Raises :class:`EnvOverrideError` if a variable targets a key the schema does
    not define, or if its value does not parse as the declared type.
    """
    applied: list[str] = []

    for var in sorted(env):
        if not var.startswith(ENV_VALUE_PREFIX):
            continue
        segments = [part.lower() for part in var[len(ENV_VALUE_PREFIX) :].split(ENV_PATH_SEPARATOR)]
        key = ".".join(segments)

        if any(not segment for segment in segments):
            raise EnvOverrideError(
                f"environment variable `{var}` targets unknown config key `{key}`"
            )

        parent = _resolve_parent(document, segments)
        leaf = segments[-1]
        if parent is None or leaf not in parent:
            raise EnvOverrideError(
                f"environment variable `{var}` targets unknown config key `{key}`"
            )

        parent[leaf] = _coerce_like(parent[leaf], env[var], var, key)
        applied.append(key)

    return sorted(applied)


def _resolve_parent(document: dict[str, Any], segments: list[str]) -> dict[str, Any] | None:
    """Walk to the table containing the leaf, or ``None`` if the path is bogus."""
    cursor: dict[str, Any] = document
    for segment in segments[:-1]:
        child = cursor.get(segment)
        if not isinstance(child, dict):
            return None
        cursor = child
    return cursor


def _ascii_lower(raw: str) -> str:
    """Lowercase ASCII letters only.

    Mirrors Rust's ``str::to_ascii_lowercase``. Python's ``str.lower`` is
    Unicode-aware, which could widen the accepted boolean spellings beyond what
    Rust accepts.
    """
    return "".join(chr(ord(ch) + 32) if "A" <= ch <= "Z" else ch for ch in raw)


def _canonical_int(raw: str) -> int | None:
    """Parse ``raw`` under the canonical integer grammar, or return ``None``.

    ``fullmatch`` (not ``$``) because ``$`` also matches before a trailing
    newline, and ``[0-9]`` (not ``\\d``) because ``\\d`` matches Unicode digits
    that Rust rejects.
    """
    if _INTEGER_RE.fullmatch(raw) is None:
        return None
    parsed = int(raw)
    # Mirrors `i64::from_str` rejecting overflow after the grammar matches.
    return parsed if _TOML_INT_MIN <= parsed <= _TOML_INT_MAX else None


def _canonical_float(raw: str) -> float | None:
    """Parse ``raw`` under the canonical float grammar, or return ``None``.

    Non-finite spellings parse here and are then rejected by value in
    :meth:`Config.validate`, identically in both languages.
    """
    if _FLOAT_RE.fullmatch(raw) is None:
        return None
    return float(raw)


def _coerce_like(existing: object, raw: str, var: str, key: str) -> object:
    """Parse ``raw`` as the same type ``existing`` already holds.

    Uses the canonical grammars above rather than ``int()``/``float()``, whose
    leniency does not match Rust's. See the grammar note near the top of this
    module.
    """
    expected = _type_name(existing)

    def reject() -> EnvOverrideError:
        return EnvOverrideError(
            f"environment variable `{var}` is not a valid {expected} for config key "
            f"`{key}` (got `{raw}`){_whitespace_hint(existing, raw)}"
        )

    # bool is checked before int: it is an int subclass in Python.
    if isinstance(existing, bool):
        lowered = _ascii_lower(raw)
        if lowered in _TRUTHY:
            return True
        if lowered in _FALSY:
            return False
        raise reject()
    if isinstance(existing, str):
        # Taken verbatim — no trimming, no interpretation — as in Rust.
        return raw
    if isinstance(existing, int):
        parsed_int = _canonical_int(raw)
        if parsed_int is None:
            raise reject()
        return parsed_int
    if isinstance(existing, float):
        parsed_float = _canonical_float(raw)
        if parsed_float is None:
            raise reject()
        return parsed_float
    # Tables and arrays are not overridable one-variable-at-a-time; point the
    # operator at a config file instead of inventing a mini-syntax.
    raise reject()


def _whitespace_hint(existing: object, raw: str) -> str:
    """Guidance for the common near-miss: a value that would have parsed if it
    were not padded with whitespace.

    Neither language trims overrides, so this turns a confusing rejection into
    an obvious one. The Rust mirror emits the same sentence.
    """
    trimmed = raw.strip()
    if trimmed == raw:
        return ""
    if isinstance(existing, bool):
        would_parse = _ascii_lower(trimmed) in _TRUTHY or _ascii_lower(trimmed) in _FALSY
    elif isinstance(existing, int):
        would_parse = _canonical_int(trimmed) is not None
    elif isinstance(existing, float):
        would_parse = _canonical_float(trimmed) is not None
    else:
        would_parse = False
    return " — surrounding whitespace is not allowed" if would_parse else ""


def _type_name(value: object) -> str:
    """The schema type name used in override error messages."""
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, str):
        return "string"
    if isinstance(value, int):
        return "integer"
    if isinstance(value, float):
        return "float"
    if isinstance(value, list):
        return "array"
    if isinstance(value, dict):
        return "table"
    return type(value).__name__
