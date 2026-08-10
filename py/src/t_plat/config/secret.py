"""Secret references, redacted secret values, and the sources they come from.

The rule this module enforces: **a config file never contains a secret**. It
contains a :class:`SecretRef` — a pointer to the environment or a secret store
— and the loader dereferences it at load time. A literal in a config file is
rejected during parsing, so a committed credential fails the build.

Mirrors ``rust/crates/platform-config/src/secret.rs``.
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from enum import StrEnum
from pathlib import Path
from typing import Protocol, runtime_checkable

from t_plat.config.errors import MissingSecretError, SecretStoreError

__all__ = [
    "SECRET_REF_SYNTAX",
    "MappingSecretSource",
    "OsSecretSource",
    "ResolvedSecrets",
    "Secret",
    "SecretKind",
    "SecretPolicy",
    "SecretRef",
    "SecretSource",
]

ENV_PREFIX = "env:"
FILE_PREFIX = "file:"
NONE_LITERAL = "none"

#: Human-readable syntax summary, reused in error messages and docs.
SECRET_REF_SYNTAX = "`env:NAME`, `file:/path`, or `none`"


class Secret:
    """A resolved secret value.

    ``repr()`` and ``str()`` both render ``Secret(<redacted>)``, so a secret
    cannot reach a log line or a traceback by accident. Reading the material
    takes an explicit, greppable :meth:`expose` call.
    """

    __slots__ = ("_value",)

    def __init__(self, value: str) -> None:
        self._value = value

    def expose(self) -> str:
        """Return the secret material.

        Call this at the point of use (opening a connection, signing a
        request) and never store the result.
        """
        return self._value

    def __len__(self) -> int:
        return len(self._value)

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Secret):
            return NotImplemented
        return self._value == other._value

    def __hash__(self) -> int:
        return hash(self._value)

    def __repr__(self) -> str:
        return "Secret(<redacted>)"

    def __str__(self) -> str:
        return "Secret(<redacted>)"


class SecretKind(StrEnum):
    """Which scheme a :class:`SecretRef` uses."""

    NONE = "none"
    """No secret configured. Allowed under ``local``, rejected under ``prod``."""

    ENV = "env"
    """Read from an environment variable."""

    FILE = "file"
    """Read from a secret-store mount (Docker/Compose or Kubernetes secrets)."""


class SecretPolicy(StrEnum):
    """How strict the loader is about secrets that do not resolve.

    Selected by the active profile; this is the concrete local-vs-prod
    behavioural split.
    """

    PERMISSIVE = "permissive"
    """``local``: an unresolvable reference yields ``None`` so a developer can
    boot without provisioning every credential."""

    REQUIRED = "required"
    """``prod``: every declared secret must resolve or the process refuses to
    start. A missing credential is a boot failure, not a runtime surprise."""


@runtime_checkable
class SecretSource(Protocol):
    """Where secret material is actually fetched from."""

    def env_var(self, name: str) -> str | None:
        """Return environment variable ``name``, or ``None`` if unset."""

    def read_file(self, path: Path) -> str:
        """Read a secret-store file.

        Raises :class:`FileNotFoundError` when the secret is simply absent,
        and any other :class:`OSError` for a broken mount.
        """


class OsSecretSource:
    """The real source: the process environment plus the filesystem."""

    def env_var(self, name: str) -> str | None:
        """Return environment variable ``name``, or ``None`` if unset."""
        return os.environ.get(name)

    def read_file(self, path: Path) -> str:
        """Read ``path`` from the filesystem."""
        return path.read_text(encoding="utf-8")


@dataclass(frozen=True, slots=True)
class MappingSecretSource:
    """An in-memory secret source for tests and embedding.

    Lets a test exercise ``prod``'s required-secret policy — including
    ``file:`` references to secret-store mounts that do not exist on the test
    machine — without touching the real environment or filesystem.
    """

    env: dict[str, str] = field(default_factory=dict)
    files: dict[str, str] = field(default_factory=dict)

    def env_var(self, name: str) -> str | None:
        """Return the registered value for ``name``, or ``None``."""
        return self.env.get(name)

    def read_file(self, path: Path) -> str:
        """Return the registered contents for ``path``."""
        try:
            return self.files[str(path)]
        except KeyError as exc:
            raise FileNotFoundError(f"no secret registered at `{path}`") from exc


@dataclass(frozen=True, slots=True)
class SecretRef:
    """Where a secret comes from, as written in a config file.

    This is the only shape a secret-typed field will parse from; see the
    module docstring.
    """

    kind: SecretKind
    """Which scheme this reference uses."""

    target: str
    """The environment variable name, or the secret-store path. Empty for
    :attr:`SecretKind.NONE`."""

    @classmethod
    def none(cls) -> SecretRef:
        """Build the "no secret configured" reference."""
        return cls(SecretKind.NONE, "")

    @classmethod
    def env(cls, name: str) -> SecretRef:
        """Build an ``env:NAME`` reference."""
        return cls(SecretKind.ENV, name)

    @classmethod
    def file(cls, path: str) -> SecretRef:
        """Build a ``file:/path`` reference."""
        return cls(SecretKind.FILE, path)

    @classmethod
    def parse(cls, raw: str) -> SecretRef:
        """Parse a reference from its textual form.

        Raises :class:`ValueError` describing why ``raw`` is not a valid
        reference. A value with no recognised scheme is reported as a
        committed literal.
        """
        if raw == NONE_LITERAL:
            return cls.none()
        if raw.startswith(ENV_PREFIX):
            name = raw[len(ENV_PREFIX) :]
            if not name:
                raise ValueError("`env:` reference is missing a variable name")
            return cls.env(name)
        if raw.startswith(FILE_PREFIX):
            path = raw[len(FILE_PREFIX) :]
            if not path:
                raise ValueError("`file:` reference is missing a path")
            return cls.file(path)
        raise ValueError(
            f"expected a secret reference ({SECRET_REF_SYNTAX}), found a literal "
            "value — secrets are never committed to config files"
        )

    def as_string(self) -> str:
        """Render back to the textual form used in config files."""
        if self.kind is SecretKind.NONE:
            return NONE_LITERAL
        return f"{self.kind.value}:{self.target}"

    def __str__(self) -> str:
        return self.as_string()

    def resolve(
        self,
        key: str,
        source: SecretSource,
        policy: SecretPolicy,
    ) -> Secret | None:
        """Resolve this reference against ``source`` under ``policy``.

        ``key`` is the dotted config key (e.g. ``database.password``) and is
        used only for error messages.

        Under :attr:`SecretPolicy.REQUIRED` an unresolvable reference raises
        :class:`MissingSecretError`; under :attr:`SecretPolicy.PERMISSIVE` it
        returns ``None``. A ``file:`` reference that exists but cannot be read
        raises :class:`SecretStoreError` under either policy — that is a broken
        mount, not an absent secret.
        """

        def missing(reason: str) -> Secret | None:
            if policy is SecretPolicy.PERMISSIVE:
                return None
            raise MissingSecretError(
                f"secret `{key}` is required under the `prod` profile but {reason}"
            )

        if self.kind is SecretKind.NONE:
            return missing(f"it is set to `{NONE_LITERAL}` (expected {SECRET_REF_SYNTAX})")

        if self.kind is SecretKind.ENV:
            value = source.env_var(self.target)
            if value:
                return Secret(value)
            return missing(f"environment variable `{self.target}` is unset or empty")

        path = Path(self.target)
        try:
            contents = source.read_file(path)
        except FileNotFoundError:
            return missing(f"secret file `{path}` does not exist")
        except OSError as exc:
            raise SecretStoreError(
                f"cannot read secret file `{path}` for config key `{key}`: {exc}"
            ) from exc

        value = contents.rstrip("\r\n")  # secret mounts routinely carry a newline
        if not value:
            return missing(f"secret file `{path}` is empty")
        return Secret(value)


@dataclass(frozen=True, slots=True)
class ResolvedSecrets:
    """The secrets resolved for one config, keyed by dotted config key.

    Holds resolved :class:`Secret` values only; keys whose reference did not
    resolve under a permissive policy are simply absent.
    """

    entries: dict[str, Secret] = field(default_factory=dict)

    def get(self, key: str) -> Secret | None:
        """Look a secret up by dotted config key (e.g. ``database.password``)."""
        return self.entries.get(key)

    def keys(self) -> list[str]:
        """The dotted keys that resolved, sorted."""
        return sorted(self.entries)

    def __len__(self) -> int:
        return len(self.entries)

    def __contains__(self, key: object) -> bool:
        return key in self.entries

    def database_password(self) -> Secret | None:
        """The resolved ``database.password``, if any."""
        return self.get("database.password")

    def market_data_api_key(self) -> Secret | None:
        """The resolved ``market_data.api_key``, if any."""
        return self.get("market_data.api_key")
