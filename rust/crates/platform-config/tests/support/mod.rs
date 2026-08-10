//! Shared helpers for the `platform-config` integration tests.
//!
//! Lives in a subdirectory so Cargo treats it as a module rather than as an
//! extra test binary.

// This module is compiled into every test binary, and no single binary uses
// all of it.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// The repository root, derived from this crate's manifest directory.
///
/// Tests resolve `config/` through this rather than the working directory,
/// because `just test` runs cargo from `rust/`.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repo root resolves")
}

/// The committed `config/` directory.
pub fn repo_config_dir() -> PathBuf {
    repo_root().join("config")
}

/// The committed sample config that the tests load.
pub fn example_config_path() -> PathBuf {
    repo_config_dir().join("config.example.toml")
}

/// Counter making temp directory names unique within a test binary.
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A self-cleaning scratch directory for config fixtures written at runtime.
///
/// Fixtures that must contain a *rejected* literal are built here rather than
/// committed, so nothing credential-shaped ever lands in the repository.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Create a fresh directory named after `label`.
    pub fn new(label: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "platform-config-{label}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("temp dir is creatable");
        Self { path }
    }

    /// Write `contents` to `name` inside the directory and return its path.
    pub fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.path.join(name);
        std::fs::write(&path, contents).expect("fixture is writable");
        path
    }

    /// The directory itself.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Flatten a TOML document into sorted dotted keys (`app.log_level`, ...).
///
/// Used to assert that the sample config and the schema hold exactly the same
/// set of keys.
pub fn flatten_keys(value: &toml::Value, prefix: &str, out: &mut Vec<String>) {
    match value {
        toml::Value::Table(table) => {
            for (key, child) in table {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_keys(child, &path, out);
            }
        }
        _ => out.push(prefix.to_owned()),
    }
    out.sort();
}
