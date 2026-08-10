"""Numeric-bounds parity: the Python half.

Every case in ``config/testdata/numeric_bounds.toml`` is driven through the
environment-override path here, and through the identical path in
``rust/crates/platform-config/tests/parity_bounds.rs``. Both suites assert the
same accept/reject outcome per case, so the two implementations of one schema
cannot disagree about which configs are valid.
"""

from __future__ import annotations

import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import pytest

from t_plat.config import (
    MAX_DATABASE_PORT,
    MAX_MARKET_DATA_MAX_RETRIES,
    MAX_MARKET_DATA_TIMEOUT_MS,
    MAX_TELEMETRY_SAMPLE_RATE,
    MIN_DATABASE_PORT,
    MIN_MARKET_DATA_MAX_RETRIES,
    MIN_MARKET_DATA_TIMEOUT_MS,
    MIN_TELEMETRY_SAMPLE_RATE,
    Config,
    ConfigError,
    MappingSecretSource,
    Profile,
    load_config,
)

REPO_ROOT = Path(__file__).resolve().parents[2]
FIXTURE = REPO_ROOT / "config" / "testdata" / "numeric_bounds.toml"
NO_SUCH_DIR = Path("/nonexistent/config/dir")

#: The numeric fields the fixture must keep covered, and how to read each back.
FIELD_READERS = {
    "T_PLAT__DATABASE__PORT": lambda c: c.database.port,
    "T_PLAT__MARKET_DATA__TIMEOUT_MS": lambda c: c.market_data.timeout_ms,
    "T_PLAT__MARKET_DATA__MAX_RETRIES": lambda c: c.market_data.max_retries,
    "T_PLAT__TELEMETRY__SAMPLE_RATE": lambda c: c.telemetry.sample_rate,
}


@dataclass(frozen=True, slots=True)
class Case:
    """One boundary case as written in the shared fixture."""

    name: str
    var: str
    value: str
    accept: bool
    why: str
    expect_int: int | None = None
    expect_float: float | None = None


def _load_cases() -> list[Case]:
    document = tomllib.loads(FIXTURE.read_text(encoding="utf-8"))
    cases: list[Case] = []
    for raw in document["case"]:
        unknown = set(raw) - {f.name for f in Case.__dataclass_fields__.values()}
        assert not unknown, f"fixture case `{raw.get('name')}` has unknown keys: {sorted(unknown)}"
        cases.append(Case(**raw))
    return cases


CASES = _load_cases()


def _load_with_override(case: Case) -> Config:
    """Apply one override to the built-in defaults, raising on rejection.

    ``config_dir`` points at nothing, so layer 1 is the only baseline and the
    case's variable is the single thing under test.
    """
    return load_config(
        env={case.var: case.value},
        profile=Profile.LOCAL,
        config_dir=NO_SUCH_DIR,
        secret_source=MappingSecretSource(),
    ).config


@pytest.mark.parametrize("case", CASES, ids=lambda case: case.name)
def test_fixture_case_matches_the_declared_outcome(case: Case) -> None:
    if not case.accept:
        # Rejection may surface as an env-parse, schema or range error; what
        # must match Rust is that the config does not load at all.
        with pytest.raises(ConfigError):
            _load_with_override(case)
        return

    config = _load_with_override(case)
    observed: Any = FIELD_READERS[case.var](config)
    if case.expect_int is not None:
        assert observed == case.expect_int, f"`{case.name}`: {case.why}"
    if case.expect_float is not None:
        assert observed == pytest.approx(case.expect_float), f"`{case.name}`: {case.why}"


def test_the_fixture_covers_every_numeric_field() -> None:
    for var in FIELD_READERS:
        covered = [case for case in CASES if case.var == var]
        assert len(covered) >= 4, f"`{var}` needs boundary coverage, found {len(covered)} cases"


def test_case_names_are_unique() -> None:
    # Names identify a case in a failure message, so duplicates would hide one.
    names = [case.name for case in CASES]
    assert len(set(names)) == len(names), "fixture case names must be unique"


def test_the_declared_constants_are_the_ones_rust_declares() -> None:
    # Mirrored verbatim in rust/crates/platform-config/src/model.rs. If either
    # side moves a bound without the other, this and its Rust twin disagree.
    assert MIN_DATABASE_PORT == 1
    assert MAX_DATABASE_PORT == 65535
    assert MIN_MARKET_DATA_TIMEOUT_MS == 1
    assert MAX_MARKET_DATA_TIMEOUT_MS == 9223372036854775807
    assert MIN_MARKET_DATA_MAX_RETRIES == 0
    assert MAX_MARKET_DATA_MAX_RETRIES == 4294967295
    assert MIN_TELEMETRY_SAMPLE_RATE == 0.0
    assert MAX_TELEMETRY_SAMPLE_RATE == 1.0


def test_the_fixture_is_shared_with_the_rust_suite() -> None:
    """The Rust suite must read this same file, not a Python-only copy."""
    rust_test = (
        REPO_ROOT / "rust" / "crates" / "platform-config" / "tests" / "parity_bounds.rs"
    ).read_text(encoding="utf-8")
    support = (
        REPO_ROOT / "rust" / "crates" / "platform-config" / "tests" / "support" / "mod.rs"
    ).read_text(encoding="utf-8")

    assert "bounds_fixture_path" in rust_test, "the Rust parity suite must load the fixture"
    assert "testdata/numeric_bounds.toml" in support, (
        "the Rust suite must point at config/testdata/numeric_bounds.toml"
    )
