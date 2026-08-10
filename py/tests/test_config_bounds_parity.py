"""Cross-language parity: the Python half.

Every case in ``config/testdata/numeric_bounds.toml`` (value ranges) and
``config/testdata/lexical_overrides.toml`` (override string grammar) is driven
through the environment-override path here, and through the identical path in
``rust/crates/platform-config/tests/parity_bounds.rs``. Both suites assert the
same accept/reject outcome per case, so the two implementations of one schema
cannot disagree about which configs are valid.
"""

from __future__ import annotations

import tomllib
from collections.abc import Callable
from dataclasses import dataclass, fields
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
TESTDATA = REPO_ROOT / "config" / "testdata"
#: The shared parity fixtures, in load order. Both are also read by the Rust
#: suite, which is what keeps the two implementations honest.
FIXTURES = (TESTDATA / "numeric_bounds.toml", TESTDATA / "lexical_overrides.toml")
NO_SUCH_DIR = Path("/nonexistent/config/dir")

#: Every override-coerced key, and how to read each back off a loaded config.
#: Integer, float, boolean and string all go through the coercion, so all four
#: need parity coverage — not just the numerics.
FIELD_READERS: dict[str, Callable[[Config], Any]] = {
    "T_PLAT__DATABASE__PORT": lambda c: c.database.port,
    "T_PLAT__MARKET_DATA__TIMEOUT_MS": lambda c: c.market_data.timeout_ms,
    "T_PLAT__MARKET_DATA__MAX_RETRIES": lambda c: c.market_data.max_retries,
    "T_PLAT__TELEMETRY__SAMPLE_RATE": lambda c: c.telemetry.sample_rate,
    "T_PLAT__APP__DEBUG": lambda c: c.app.debug,
    "T_PLAT__APP__LOG_LEVEL": lambda c: c.app.log_level,
}

#: Spellings Python's int()/float()/str.strip accept and Rust's parse/trim
#: reject. If a case is ever dropped from the fixture, the class of bug it
#: guards silently comes back.
KNOWN_STDLIB_DIVERGENCES = ("1_0", "0.5_0", "\u001f10", "\u0661\u0660", "\uff11\uff10")


@dataclass(frozen=True, slots=True)
class Case:
    """One parity case as written in a shared fixture."""

    name: str
    var: str
    value: str
    accept: bool
    why: str
    expect_int: int | None = None
    expect_float: float | None = None
    expect_bool: bool | None = None
    expect_string: str | None = None


def _load_cases() -> list[Case]:
    known = {field.name for field in fields(Case)}
    cases: list[Case] = []
    for fixture in FIXTURES:
        document = tomllib.loads(fixture.read_text(encoding="utf-8"))
        for raw in document["case"]:
            unknown = set(raw) - known
            assert not unknown, (
                f"{fixture.name}: case `{raw.get('name')}` has unknown keys: {sorted(unknown)}"
            )
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
    observed = FIELD_READERS[case.var](config)
    context = f"`{case.name}`: {case.why}"

    if case.expect_int is not None:
        assert observed == case.expect_int, context
    if case.expect_float is not None:
        assert observed == pytest.approx(case.expect_float), context
    if case.expect_bool is not None:
        assert observed is case.expect_bool, context
    if case.expect_string is not None:
        assert observed == case.expect_string, context


def test_the_fixtures_cover_every_overridable_type() -> None:
    for var in FIELD_READERS:
        covered = [case for case in CASES if case.var == var]
        assert len(covered) >= 2, f"`{var}` needs parity coverage, found {len(covered)} cases"


def test_the_lexical_fixture_pins_the_known_stdlib_divergences() -> None:
    for value in KNOWN_STDLIB_DIVERGENCES:
        assert any(case.value == value and not case.accept for case in CASES), (
            f"the fixture must keep a rejecting case for {value!r}"
        )


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


def test_the_fixtures_are_shared_with_the_rust_suite() -> None:
    """The Rust suite must read these same files, not Python-only copies."""
    crate = REPO_ROOT / "rust" / "crates" / "platform-config" / "tests"
    rust_test = (crate / "parity_bounds.rs").read_text(encoding="utf-8")
    support = (crate / "support" / "mod.rs").read_text(encoding="utf-8")

    assert "parity_fixture_paths" in rust_test, "the Rust parity suite must load the fixtures"
    for fixture in FIXTURES:
        assert f"testdata/{fixture.name}" in support or fixture.name in support, (
            f"the Rust suite must point at config/testdata/{fixture.name}"
        )
