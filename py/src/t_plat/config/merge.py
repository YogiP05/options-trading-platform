"""Layer merging and typed environment overrides.

Config layers are merged as plain TOML documents before they are parsed into
the schema, which is what lets a higher layer state only the keys it changes.

Mirrors ``rust/crates/platform-config/src/merge.rs``.
"""

from __future__ import annotations

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


def _coerce_like(existing: object, raw: str, var: str, key: str) -> object:
    """Parse ``raw`` as the same type ``existing`` already holds."""
    expected = _type_name(existing)

    def reject() -> EnvOverrideError:
        return EnvOverrideError(
            f"environment variable `{var}` is not a valid {expected} for config key "
            f"`{key}` (got `{raw}`)"
        )

    # bool is checked before int: it is an int subclass in Python.
    if isinstance(existing, bool):
        lowered = raw.strip().lower()
        if lowered in _TRUTHY:
            return True
        if lowered in _FALSY:
            return False
        raise reject()
    if isinstance(existing, str):
        return raw
    if isinstance(existing, int):
        try:
            return int(raw.strip())
        except ValueError as exc:
            raise reject() from exc
    if isinstance(existing, float):
        try:
            return float(raw.strip())
        except ValueError as exc:
            raise reject() from exc
    # Tables and arrays are not overridable one-variable-at-a-time; point the
    # operator at a config file instead of inventing a mini-syntax.
    raise reject()


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
