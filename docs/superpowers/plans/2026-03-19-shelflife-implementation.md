# shelflife Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust CLI that scans a project directory for npm dependency and runtime EOL risk, outputting findings to terminal or JSON.

**Architecture:** Three-layer detection: file parsers emit typed `Fact` values, a resolver deduplicates and picks the best runtime source, then registries (npm, endoflife.date) are queried to generate severity-tagged signals. Output is either a colored terminal table or JSON.

**Tech Stack:** Rust 2021 edition, clap (CLI), reqwest + tokio (async HTTP), semver, serde/serde_json, toml, owo-colors, chrono.

**Spec:** `docs/superpowers/specs/2026-03-19-shelflife-design.md`

**Pre-commit checks:** `cargo fmt -- --check && cargo clippy -- -D warnings && cargo test`

**Test fixtures directory:** `tests/fixtures/` — all parsers use fixture files, not inline strings, so tests exercise real file I/O.

---

### Task 1: Project Scaffold + Model Types

Set up the Cargo project, dependencies, and core data types. Everything else depends on this.

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/model.rs`

- [ ] **Step 1: Initialize the Cargo project and create `.gitignore`**

```bash
cd /Users/rroskam/repos/shelflife
cargo init --name shelflife
echo '/target' > .gitignore
```

- [ ] **Step 2: Replace `Cargo.toml` with full dependency list**

```toml
[package]
name = "shelflife"
version = "0.1.0"
edition = "2021"

[dependencies]
clap = { version = "4", features = ["derive"] }
reqwest = { version = "0.12", features = ["json"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
semver = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
owo-colors = "4"
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Write tests for model types**

Create `src/model.rs` with tests first. The types are data-only (no logic) so tests verify construction and serialization.

```rust
// src/model.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_severity_mapping() {
        assert_eq!(SignalKind::Deprecated.severity(), Severity::Critical);
        assert_eq!(SignalKind::Eol.severity(), Severity::Critical);
        assert_eq!(SignalKind::Stale.severity(), Severity::Warning);
        assert_eq!(SignalKind::BehindMajor.severity(), Severity::Warning);
        assert_eq!(SignalKind::BehindMinor.severity(), Severity::Warning);
        assert_eq!(SignalKind::ApproachingEol.severity(), Severity::Warning);
        assert_eq!(SignalKind::RegistryError.severity(), Severity::Info);
        assert_eq!(SignalKind::NotFound.severity(), Severity::Info);
    }

    #[test]
    fn scan_result_serializes_to_json() {
        let result = ScanResult {
            findings: vec![],
            counts: Counts { total: 0, critical: 0, warning: 0, ok: 0 },
            scanned_at: Utc::now(),
            path: PathBuf::from("."),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"findings\":[]"));
    }
}
```

- [ ] **Step 4: Implement model types to pass tests**

```rust
// src/model.rs
use std::path::PathBuf;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

// -- Facts (parser output) --

#[derive(Debug, Clone)]
pub enum Fact {
    Dependency { name: String, version: String },
    RuntimeVersion { runtime: Runtime, version: String, source: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    #[serde(rename = "nodejs")]
    NodeJs,
    Python,
    Java,
}

impl Runtime {
    /// The product slug used by the endoflife.date API.
    pub fn eol_slug(&self) -> &'static str {
        match self {
            Runtime::NodeJs => "nodejs",
            Runtime::Python => "python",
            Runtime::Java => "java",
        }
    }
}

// -- Findings (output) --

#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub findings: Vec<Finding>,
    pub counts: Counts,
    pub scanned_at: DateTime<Utc>,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct Counts {
    pub total: usize,
    pub critical: usize,
    pub warning: usize,
    pub ok: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Ecosystem {
    Npm,
    Runtime,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub ecosystem: Ecosystem,
    pub name: String,
    pub installed_version: String,
    pub latest_version: Option<String>,
    pub signals: Vec<Signal>,
    pub eol_info: Option<EolInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Signal {
    pub kind: SignalKind,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    Deprecated,
    Stale,
    BehindMajor,
    BehindMinor,
    Eol,
    ApproachingEol,
    RegistryError,
    NotFound,
}

impl SignalKind {
    pub fn severity(&self) -> Severity {
        match self {
            SignalKind::Deprecated | SignalKind::Eol => Severity::Critical,
            SignalKind::Stale | SignalKind::BehindMajor | SignalKind::BehindMinor | SignalKind::ApproachingEol => Severity::Warning,
            SignalKind::RegistryError | SignalKind::NotFound => Severity::Info,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize)]
pub struct EolInfo {
    pub eol_date: Option<NaiveDate>,
    pub days_left: Option<i64>,
    pub cycle: String,
    pub ref_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FailOn {
    Any,
    Critical,
    None,
}
```

- [ ] **Step 5: Wire up `main.rs` with module declaration**

```rust
// src/main.rs
mod model;

fn main() {
    println!("shelflife v0.1.0");
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 7: Run pre-commit checks and commit**

```bash
cargo fmt -- --check && cargo clippy -- -D warnings && cargo test
git add .gitignore Cargo.toml Cargo.lock src/main.rs src/model.rs
git commit -m "feat: scaffold project with core model types"
```

---

### Task 2: Config Loading

Config merges defaults → `.shelflife.toml` → CLI flags. CLI parsing and TOML loading.

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs`
- Create: `tests/fixtures/config/shelflife.toml`

- [ ] **Step 1: Create test fixture**

Create `tests/fixtures/config/shelflife.toml`:

```toml
threshold_days = 90
stale_months = 12
ignore = ["internal-pkg"]
fail_on = "critical"
```

- [ ] **Step 2: Write tests for config loading**

```rust
// src/config.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn defaults() {
        let config = Config::default();
        assert_eq!(config.threshold_days, 180);
        assert_eq!(config.stale_months, 18);
        assert!(config.ignore.is_empty());
        assert_eq!(config.fail_on, FailOn::Any);
        assert!(!config.json);
        assert!(!config.verbose);
    }

    #[test]
    fn from_toml_file() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/config/shelflife.toml");
        let config = Config::from_toml(&path).unwrap();
        assert_eq!(config.threshold_days, 90);
        assert_eq!(config.stale_months, 12);
        assert_eq!(config.ignore, vec!["internal-pkg"]);
        assert_eq!(config.fail_on, FailOn::Critical);
    }

    #[test]
    fn merge_overrides_toml_with_cli() {
        let file_config = Config {
            threshold_days: 90,
            stale_months: 12,
            ..Config::default()
        };
        let cli = CliOverrides {
            threshold_days: Some(30),
            stale_months: None,
            ignore: None,
            fail_on: None,
            json: false,
            verbose: false,
        };
        let merged = file_config.merge(cli);
        assert_eq!(merged.threshold_days, 30);  // CLI wins
        assert_eq!(merged.stale_months, 12);    // TOML preserved
    }

    #[test]
    fn missing_toml_returns_error() {
        let result = Config::from_toml(Path::new("/nonexistent/.shelflife.toml"));
        assert!(result.is_err());
    }
}
```

- [ ] **Step 3: Implement Config**

```rust
// src/config.rs
use std::path::{Path, PathBuf};
use serde::Deserialize;
use crate::model::FailOn;

#[derive(Debug, Clone)]
pub struct Config {
    pub threshold_days: u32,
    pub stale_months: u32,
    pub ignore: Vec<String>,
    pub fail_on: FailOn,
    pub json: bool,
    pub verbose: bool,
    pub path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            threshold_days: 180,
            stale_months: 18,
            ignore: vec![],
            fail_on: FailOn::Any,
            json: false,
            verbose: false,
            path: PathBuf::from("."),
        }
    }
}

/// Intermediate struct for TOML deserialization (all fields optional).
#[derive(Debug, Deserialize)]
struct TomlConfig {
    threshold_days: Option<u32>,
    stale_months: Option<u32>,
    ignore: Option<Vec<String>>,
    fail_on: Option<FailOn>,
}

/// CLI overrides — None means "not specified, use file/default".
#[derive(Debug)]
pub struct CliOverrides {
    pub threshold_days: Option<u32>,
    pub stale_months: Option<u32>,
    pub ignore: Option<Vec<String>>,
    pub fail_on: Option<FailOn>,
    pub json: bool,
    pub verbose: bool,
}

impl Config {
    pub fn from_toml(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let toml_config: TomlConfig = toml::from_str(&content)?;
        let mut config = Config::default();
        if let Some(v) = toml_config.threshold_days { config.threshold_days = v; }
        if let Some(v) = toml_config.stale_months { config.stale_months = v; }
        if let Some(v) = toml_config.ignore { config.ignore = v; }
        if let Some(v) = toml_config.fail_on { config.fail_on = v; }
        Ok(config)
    }

    pub fn merge(mut self, cli: CliOverrides) -> Self {
        if let Some(v) = cli.threshold_days { self.threshold_days = v; }
        if let Some(v) = cli.stale_months { self.stale_months = v; }
        if let Some(v) = cli.ignore { self.ignore = v; }
        if let Some(v) = cli.fail_on { self.fail_on = v; }
        self.json = cli.json;
        self.verbose = cli.verbose;
        self
    }
}
```

- [ ] **Step 4: Add clap CLI definition to `main.rs`**

```rust
// src/main.rs
mod config;
mod model;

use clap::Parser;
use std::path::PathBuf;
use config::{CliOverrides, Config};
use model::FailOn;

#[derive(Parser)]
#[command(name = "shelflife", version, about = "Check dependencies and runtimes for end-of-life risk")]
struct Cli {
    /// Directory to scan
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Config file path
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Days before EOL to warn
    #[arg(long)]
    threshold_days: Option<u32>,

    /// Months without update = stale
    #[arg(long)]
    stale_months: Option<u32>,

    /// Packages to skip (comma-separated)
    #[arg(long, value_delimiter = ',')]
    ignore: Option<Vec<String>>,

    /// Output JSON instead of table
    #[arg(long)]
    json: bool,

    /// Exit 1 on: any, critical, none
    #[arg(long, value_parser = parse_fail_on)]
    fail_on: Option<FailOn>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

fn parse_fail_on(s: &str) -> Result<FailOn, String> {
    match s {
        "any" => Ok(FailOn::Any),
        "critical" => Ok(FailOn::Critical),
        "none" => Ok(FailOn::None),
        _ => Err(format!("invalid value '{s}', expected: any, critical, none")),
    }
}

fn main() {
    let cli = Cli::parse();

    let config_path = cli.config.clone()
        .unwrap_or_else(|| cli.path.join(".shelflife.toml"));

    let file_config = if config_path.exists() {
        Config::from_toml(&config_path).unwrap_or_else(|e| {
            eprintln!("warning: failed to read config {}: {e}", config_path.display());
            Config::default()
        })
    } else {
        Config::default()
    };

    let overrides = CliOverrides {
        threshold_days: cli.threshold_days,
        stale_months: cli.stale_months,
        ignore: cli.ignore,
        fail_on: cli.fail_on,
        json: cli.json,
        verbose: cli.verbose,
    };

    let mut config = file_config.merge(overrides);
    config.path = cli.path;

    if config.verbose {
        eprintln!("config: {:?}", config);
    }
}
```

- [ ] **Step 5: Add `Deserialize` for `FailOn` in model.rs**

Add `serde::Deserialize` derive to `FailOn`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FailOn { Any, Critical, None }
```

- [ ] **Step 6: Run tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 7: Run pre-commit checks and commit**

```bash
cargo fmt -- --check && cargo clippy -- -D warnings && cargo test
git add src/config.rs src/main.rs src/model.rs tests/fixtures/config/
git commit -m "feat: config loading with TOML + CLI override merging"
```

---

### Task 3: File Parsers — Lockfile

Parse `package-lock.json` v1/v2/v3 into `Dependency` facts.

**Files:**
- Create: `src/parsers/mod.rs`
- Create: `src/parsers/lockfile.rs`
- Create: `tests/fixtures/lockfile/v1.json`
- Create: `tests/fixtures/lockfile/v2.json`
- Create: `tests/fixtures/lockfile/v3.json`

- [ ] **Step 1: Create test fixtures**

`tests/fixtures/lockfile/v1.json`:
```json
{
  "name": "test-project",
  "lockfileVersion": 1,
  "dependencies": {
    "express": { "version": "4.18.2" },
    "lodash": { "version": "4.17.21" }
  }
}
```

`tests/fixtures/lockfile/v2.json`:
```json
{
  "name": "test-project",
  "lockfileVersion": 2,
  "packages": {
    "": { "name": "test-project", "dependencies": { "express": "^4.18.0" } },
    "node_modules/express": { "version": "4.18.2" },
    "node_modules/lodash": { "version": "4.17.21" }
  },
  "dependencies": {
    "express": { "version": "4.18.2" },
    "lodash": { "version": "4.17.21" }
  }
}
```

`tests/fixtures/lockfile/v3.json`:
```json
{
  "name": "test-project",
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "test-project", "dependencies": { "express": "^4.18.0" } },
    "node_modules/express": { "version": "4.18.2" },
    "node_modules/lodash": { "version": "4.17.21" },
    "node_modules/express/node_modules/body-parser": { "version": "1.20.1" }
  }
}
```

- [ ] **Step 2: Write tests**

```rust
// src/parsers/lockfile.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/lockfile")
            .join(name)
    }

    #[test]
    fn parse_v1_lockfile() {
        let facts = parse_lockfile(&fixture("v1.json")).unwrap();
        assert_eq!(facts.len(), 2);
        assert!(facts.iter().any(|f| matches!(f, Fact::Dependency { name, version } if name == "express" && version == "4.18.2")));
        assert!(facts.iter().any(|f| matches!(f, Fact::Dependency { name, version } if name == "lodash" && version == "4.17.21")));
    }

    #[test]
    fn parse_v2_prefers_packages_field() {
        let facts = parse_lockfile(&fixture("v2.json")).unwrap();
        assert_eq!(facts.len(), 2); // top-level only, not nested
        assert!(facts.iter().any(|f| matches!(f, Fact::Dependency { name, .. } if name == "express")));
    }

    #[test]
    fn parse_v3_top_level_only() {
        let facts = parse_lockfile(&fixture("v3.json")).unwrap();
        // body-parser is nested under express, should not appear
        assert_eq!(facts.len(), 2);
        assert!(!facts.iter().any(|f| matches!(f, Fact::Dependency { name, .. } if name == "body-parser")));
    }

    #[test]
    fn missing_file_returns_error() {
        assert!(parse_lockfile(Path::new("/nonexistent/package-lock.json")).is_err());
    }
}
```

- [ ] **Step 3: Implement lockfile parser**

```rust
// src/parsers/lockfile.rs
use std::path::Path;
use serde_json::Value;
use crate::model::Fact;

pub fn parse_lockfile(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let parsed: Value = serde_json::from_str(&content)?;

    let version = parsed.get("lockfileVersion")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);

    if version >= 2 {
        if let Some(packages) = parsed.get("packages").and_then(|p| p.as_object()) {
            return Ok(parse_packages_field(packages));
        }
    }

    // v1 fallback or v2 without packages field
    if let Some(deps) = parsed.get("dependencies").and_then(|d| d.as_object()) {
        return Ok(parse_dependencies_field(deps));
    }

    Ok(vec![])
}

fn parse_packages_field(packages: &serde_json::Map<String, Value>) -> Vec<Fact> {
    packages.iter()
        .filter_map(|(key, value)| {
            // Top-level deps are "node_modules/{name}" (no nested /)
            let name = key.strip_prefix("node_modules/")?;
            if name.contains("/node_modules/") || name.is_empty() {
                return None;
            }
            let version = value.get("version")?.as_str()?.to_string();
            Some(Fact::Dependency { name: name.to_string(), version })
        })
        .collect()
}

fn parse_dependencies_field(deps: &serde_json::Map<String, Value>) -> Vec<Fact> {
    deps.iter()
        .filter_map(|(name, value)| {
            let version = value.get("version")?.as_str()?.to_string();
            Some(Fact::Dependency { name: name.clone(), version })
        })
        .collect()
}
```

- [ ] **Step 4: Create `parsers/mod.rs`**

```rust
// src/parsers/mod.rs
pub mod lockfile;
```

- [ ] **Step 5: Add `mod parsers;` to `main.rs`**

- [ ] **Step 6: Run tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 7: Run pre-commit checks and commit**

```bash
cargo fmt -- --check && cargo clippy -- -D warnings && cargo test
git add src/parsers/ tests/fixtures/lockfile/
git commit -m "feat: package-lock.json parser (v1/v2/v3)"
```

---

### Task 4: File Parsers — Node.js Runtime

Parse `.nvmrc`, `.node-version`, and `package.json` `engines.node` for Node.js version.

**Files:**
- Create: `src/parsers/nvmrc.rs`
- Create: `src/parsers/package_json.rs`
- Create: `tests/fixtures/nvmrc/` (multiple fixtures)
- Create: `tests/fixtures/package_json/`

- [ ] **Step 1: Create test fixtures**

`tests/fixtures/nvmrc/pinned`: `20.11.0`
`tests/fixtures/nvmrc/major_only`: `20`
`tests/fixtures/nvmrc/v_prefix`: `v20.11.0`
`tests/fixtures/nvmrc/lts_alias`: `lts/iron`
`tests/fixtures/nvmrc/with_comments`:
```
# This is a comment
18.19.0
```

`tests/fixtures/package_json/with_engines.json`:
```json
{
  "name": "test-project",
  "engines": { "node": ">=18" }
}
```

`tests/fixtures/package_json/caret_engines.json`:
```json
{
  "name": "test-project",
  "engines": { "node": "^20.0.0" }
}
```

`tests/fixtures/package_json/no_engines.json`:
```json
{
  "name": "test-project"
}
```

- [ ] **Step 2: Write tests for nvmrc parser**

```rust
// src/parsers/nvmrc.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fact, Runtime};
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/nvmrc")
            .join(name)
    }

    #[test]
    fn pinned_version() {
        let facts = parse_nvmrc(&fixture("pinned")).unwrap();
        assert_eq!(facts.len(), 1);
        assert!(matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::NodeJs, version, .. } if version == "20.11.0"));
    }

    #[test]
    fn major_only() {
        let facts = parse_nvmrc(&fixture("major_only")).unwrap();
        assert!(matches!(&facts[0], Fact::RuntimeVersion { version, .. } if version == "20"));
    }

    #[test]
    fn strips_v_prefix() {
        let facts = parse_nvmrc(&fixture("v_prefix")).unwrap();
        assert!(matches!(&facts[0], Fact::RuntimeVersion { version, .. } if version == "20.11.0"));
    }

    #[test]
    fn alias_returns_empty() {
        let facts = parse_nvmrc(&fixture("lts_alias")).unwrap();
        assert!(facts.is_empty());
    }

    #[test]
    fn ignores_comments() {
        let facts = parse_nvmrc(&fixture("with_comments")).unwrap();
        assert!(matches!(&facts[0], Fact::RuntimeVersion { version, .. } if version == "18.19.0"));
    }
}
```

- [ ] **Step 3: Implement nvmrc parser**

```rust
// src/parsers/nvmrc.rs
use std::path::Path;
use crate::model::{Fact, Runtime};

pub fn parse_nvmrc(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let version = content
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .unwrap_or("");

    let version = version.strip_prefix('v').unwrap_or(version);

    // Check if it's a numeric version (starts with a digit)
    if version.is_empty() || !version.starts_with(|c: char| c.is_ascii_digit()) {
        // Alias like lts/iron, node, stable — skip
        return Ok(vec![]);
    }

    Ok(vec![Fact::RuntimeVersion {
        runtime: Runtime::NodeJs,
        version: version.to_string(),
        source: path.display().to_string(),
    }])
}
```

- [ ] **Step 4: Run nvmrc tests**

Run: `cargo test parsers::nvmrc`
Expected: All pass.

- [ ] **Step 5: Write tests for package_json parser**

```rust
// src/parsers/package_json.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fact, Runtime};
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/package_json")
            .join(name)
    }

    #[test]
    fn extracts_minimum_major_from_gte_range() {
        let facts = parse_package_json_engines(&fixture("with_engines.json")).unwrap();
        assert_eq!(facts.len(), 1);
        assert!(matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::NodeJs, version, .. } if version == "18"));
    }

    #[test]
    fn extracts_major_from_caret_range() {
        let facts = parse_package_json_engines(&fixture("caret_engines.json")).unwrap();
        assert!(matches!(&facts[0], Fact::RuntimeVersion { version, .. } if version == "20"));
    }

    #[test]
    fn no_engines_returns_empty() {
        let facts = parse_package_json_engines(&fixture("no_engines.json")).unwrap();
        assert!(facts.is_empty());
    }
}
```

- [ ] **Step 6: Implement package_json parser**

```rust
// src/parsers/package_json.rs
use std::path::Path;
use serde_json::Value;
use crate::model::{Fact, Runtime};

pub fn parse_package_json_engines(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let parsed: Value = serde_json::from_str(&content)?;

    let node_range = parsed
        .get("engines")
        .and_then(|e| e.get("node"))
        .and_then(|n| n.as_str());

    let Some(range) = node_range else {
        return Ok(vec![]);
    };

    // Extract first numeric sequence as the minimum major version.
    // Handles: ">=18", "^20.0.0", "18.x", "~18.0.0", etc.
    let major: String = range
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();

    if major.is_empty() {
        return Ok(vec![]);
    }

    Ok(vec![Fact::RuntimeVersion {
        runtime: Runtime::NodeJs,
        version: major,
        source: path.display().to_string(),
    }])
}
```

- [ ] **Step 7: Register modules in `parsers/mod.rs`**

```rust
pub mod lockfile;
pub mod nvmrc;
pub mod package_json;
```

- [ ] **Step 8: Run all tests**

Run: `cargo test`
Expected: All pass.

- [ ] **Step 9: Run pre-commit checks and commit**

```bash
cargo fmt -- --check && cargo clippy -- -D warnings && cargo test
git add src/parsers/ tests/fixtures/nvmrc/ tests/fixtures/package_json/
git commit -m "feat: Node.js runtime parsers (.nvmrc, .node-version, engines.node)"
```

---

### Task 5: File Parsers — Python + Java Runtime

Parse `.python-version`, `runtime.txt`, `pyproject.toml`, `pom.xml`, `build.gradle`.

**Files:**
- Create: `src/parsers/python.rs`
- Create: `src/parsers/java.rs`
- Create: `tests/fixtures/python/`
- Create: `tests/fixtures/java/`

- [ ] **Step 1: Create Python test fixtures**

`tests/fixtures/python/python-version`: `3.12.1`
`tests/fixtures/python/runtime.txt`: `python-3.11.6`

`tests/fixtures/python/pyproject.toml`:
```toml
[project]
name = "my-project"
requires-python = ">=3.10"
```

- [ ] **Step 2: Write Python parser tests**

```rust
// src/parsers/python.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fact, Runtime};
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/python")
            .join(name)
    }

    #[test]
    fn python_version_file() {
        let facts = parse_python_version(&fixture("python-version")).unwrap();
        assert!(matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::Python, version, .. } if version == "3.12.1"));
    }

    #[test]
    fn runtime_txt() {
        let facts = parse_runtime_txt(&fixture("runtime.txt")).unwrap();
        assert!(matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::Python, version, .. } if version == "3.11.6"));
    }

    #[test]
    fn pyproject_toml() {
        let facts = parse_pyproject_toml(&fixture("pyproject.toml")).unwrap();
        assert!(matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::Python, version, .. } if version == "3.10"));
    }
}
```

- [ ] **Step 3: Implement Python parsers**

```rust
// src/parsers/python.rs
use std::path::Path;
use crate::model::{Fact, Runtime};

pub fn parse_python_version(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let version = content.trim().to_string();
    if version.is_empty() {
        return Ok(vec![]);
    }
    Ok(vec![Fact::RuntimeVersion {
        runtime: Runtime::Python,
        version,
        source: path.display().to_string(),
    }])
}

pub fn parse_runtime_txt(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let version = content.trim().strip_prefix("python-").unwrap_or("").to_string();
    if version.is_empty() {
        return Ok(vec![]);
    }
    Ok(vec![Fact::RuntimeVersion {
        runtime: Runtime::Python,
        version,
        source: path.display().to_string(),
    }])
}

pub fn parse_pyproject_toml(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let parsed: toml::Value = content.parse()?;

    let requires = parsed
        .get("project")
        .and_then(|p| p.get("requires-python"))
        .and_then(|r| r.as_str());

    let Some(specifier) = requires else {
        return Ok(vec![]);
    };

    // Extract first version number (major.minor) from specifier like ">=3.10"
    let version: String = specifier
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();

    if version.is_empty() {
        return Ok(vec![]);
    }

    Ok(vec![Fact::RuntimeVersion {
        runtime: Runtime::Python,
        version,
        source: path.display().to_string(),
    }])
}
```

- [ ] **Step 4: Run Python tests**

Run: `cargo test parsers::python`
Expected: All pass.

- [ ] **Step 5: Create Java test fixtures**

`tests/fixtures/java/pom.xml`:
```xml
<project>
  <properties>
    <maven.compiler.source>17</maven.compiler.source>
    <maven.compiler.target>17</maven.compiler.target>
  </properties>
</project>
```

`tests/fixtures/java/build.gradle`:
```
sourceCompatibility = '17'
```

`tests/fixtures/java/build_enum.gradle`:
```
sourceCompatibility = JavaVersion.VERSION_21
```

- [ ] **Step 6: Write Java parser tests**

```rust
// src/parsers/java.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fact, Runtime};
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/java")
            .join(name)
    }

    #[test]
    fn pom_xml_properties() {
        let facts = parse_pom_xml(&fixture("pom.xml")).unwrap();
        assert!(matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::Java, version, .. } if version == "17"));
    }

    #[test]
    fn build_gradle_quoted() {
        let facts = parse_build_gradle(&fixture("build.gradle")).unwrap();
        assert!(matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::Java, version, .. } if version == "17"));
    }

    #[test]
    fn build_gradle_enum() {
        let facts = parse_build_gradle(&fixture("build_enum.gradle")).unwrap();
        assert!(matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::Java, version, .. } if version == "21"));
    }
}
```

- [ ] **Step 7: Implement Java parsers**

```rust
// src/parsers/java.rs
use std::path::Path;
use crate::model::{Fact, Runtime};

pub fn parse_pom_xml(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;

    // Try <maven.compiler.source>, <maven.compiler.release>, <source>, <target>, <release>
    let tags = [
        "maven.compiler.source",
        "maven.compiler.target",
        "maven.compiler.release",
        "source",
        "target",
        "release",
    ];

    for tag in tags {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        if let Some(start) = content.find(&open) {
            let value_start = start + open.len();
            if let Some(end) = content[value_start..].find(&close) {
                let value = content[value_start..value_start + end].trim();
                // Skip property references like ${java.version}
                if value.starts_with("${") {
                    continue;
                }
                if value.chars().all(|c| c.is_ascii_digit() || c == '.') && !value.is_empty() {
                    return Ok(vec![Fact::RuntimeVersion {
                        runtime: Runtime::Java,
                        version: value.to_string(),
                        source: path.display().to_string(),
                    }]);
                }
            }
        }
    }

    Ok(vec![])
}

pub fn parse_build_gradle(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;

    // Match: sourceCompatibility = '17' or "17"
    // Match: sourceCompatibility = JavaVersion.VERSION_17
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("sourceCompatibility") {
            continue;
        }

        // Try JavaVersion.VERSION_XX
        if let Some(pos) = trimmed.find("VERSION_") {
            let version: String = trimmed[pos + 8..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !version.is_empty() {
                return Ok(vec![Fact::RuntimeVersion {
                    runtime: Runtime::Java,
                    version,
                    source: path.display().to_string(),
                }]);
            }
        }

        // Try quoted version: '17' or "17"
        let version: String = trimmed
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if !version.is_empty() {
            return Ok(vec![Fact::RuntimeVersion {
                runtime: Runtime::Java,
                version,
                source: path.display().to_string(),
            }]);
        }
    }

    Ok(vec![])
}
```

- [ ] **Step 8: Register modules in `parsers/mod.rs`**

```rust
pub mod lockfile;
pub mod nvmrc;
pub mod package_json;
pub mod python;
pub mod java;
```

- [ ] **Step 9: Run all tests**

Run: `cargo test`
Expected: All pass.

- [ ] **Step 10: Run pre-commit checks and commit**

```bash
cargo fmt -- --check && cargo clippy -- -D warnings && cargo test
git add src/parsers/ tests/fixtures/python/ tests/fixtures/java/
git commit -m "feat: Python and Java runtime parsers"
```

---

### Task 6: File Discovery + Resolver

Scan directory for known files, run parsers, resolve runtime priority.

**Files:**
- Modify: `src/parsers/mod.rs` (add discovery logic)
- Create: `src/resolver.rs`
- Create: `tests/fixtures/resolver/` (project directories)

- [ ] **Step 1: Create test fixture — a fake project directory**

`tests/fixtures/resolver/full_project/.nvmrc`: `20.11.0`

`tests/fixtures/resolver/full_project/package-lock.json`:
```json
{
  "name": "test",
  "lockfileVersion": 3,
  "packages": {
    "": {},
    "node_modules/express": { "version": "4.18.2" }
  }
}
```

`tests/fixtures/resolver/full_project/package.json`:
```json
{
  "name": "test",
  "engines": { "node": ">=18" }
}
```

`tests/fixtures/resolver/full_project/.python-version`: `3.12.1`

- [ ] **Step 2: Add file discovery to `parsers/mod.rs`**

```rust
// src/parsers/mod.rs
pub mod lockfile;
pub mod nvmrc;
pub mod package_json;
pub mod python;
pub mod java;

use std::path::Path;
use crate::model::Fact;

/// Discover known files in the directory and run all applicable parsers.
pub fn parse_directory(dir: &Path) -> Vec<Fact> {
    let mut facts = Vec::new();

    let try_parse = |file: &str, parser: fn(&Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>>| -> Vec<Fact> {
        let path = dir.join(file);
        if path.exists() {
            parser(&path).unwrap_or_default()
        } else {
            vec![]
        }
    };

    facts.extend(try_parse("package-lock.json", lockfile::parse_lockfile));
    facts.extend(try_parse(".nvmrc", nvmrc::parse_nvmrc));
    facts.extend(try_parse(".node-version", nvmrc::parse_nvmrc));
    facts.extend(try_parse("package.json", package_json::parse_package_json_engines));
    facts.extend(try_parse(".python-version", python::parse_python_version));
    facts.extend(try_parse("runtime.txt", python::parse_runtime_txt));
    facts.extend(try_parse("pyproject.toml", python::parse_pyproject_toml));
    facts.extend(try_parse("pom.xml", java::parse_pom_xml));
    facts.extend(try_parse("build.gradle", java::parse_build_gradle));

    facts
}
```

- [ ] **Step 3: Write resolver tests**

```rust
// src/resolver.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fact, Runtime};

    #[test]
    fn resolves_dependencies() {
        let facts = vec![
            Fact::Dependency { name: "express".into(), version: "4.18.2".into() },
            Fact::Dependency { name: "lodash".into(), version: "4.17.21".into() },
        ];
        let resolved = resolve(facts);
        assert_eq!(resolved.dependencies.len(), 2);
    }

    #[test]
    fn picks_nvmrc_over_engines() {
        let facts = vec![
            Fact::RuntimeVersion { runtime: Runtime::NodeJs, version: "20".into(), source: ".nvmrc".into() },
            Fact::RuntimeVersion { runtime: Runtime::NodeJs, version: "18".into(), source: "package.json".into() },
        ];
        let resolved = resolve(facts);
        let node = resolved.runtimes.get(&Runtime::NodeJs).unwrap();
        assert_eq!(node.version, "20");
    }

    #[test]
    fn picks_python_version_over_pyproject() {
        let facts = vec![
            Fact::RuntimeVersion { runtime: Runtime::Python, version: "3.12.1".into(), source: ".python-version".into() },
            Fact::RuntimeVersion { runtime: Runtime::Python, version: "3.10".into(), source: "pyproject.toml".into() },
        ];
        let resolved = resolve(facts);
        let python = resolved.runtimes.get(&Runtime::Python).unwrap();
        assert_eq!(python.version, "3.12.1");
    }

    #[test]
    fn no_runtime_if_none_detected() {
        let facts = vec![
            Fact::Dependency { name: "express".into(), version: "4.18.2".into() },
        ];
        let resolved = resolve(facts);
        assert!(resolved.runtimes.is_empty());
    }
}
```

- [ ] **Step 4: Implement resolver**

```rust
// src/resolver.rs
use std::collections::HashMap;
use crate::model::{Fact, Runtime};

/// Source priority per runtime — index in this list determines priority (lower = higher priority).
fn source_priority(runtime: &Runtime) -> &'static [&'static str] {
    match runtime {
        Runtime::NodeJs => &[".nvmrc", ".node-version", "package.json"],
        Runtime::Python => &[".python-version", "runtime.txt", "pyproject.toml"],
        Runtime::Java => &["pom.xml", "build.gradle"],
    }
}

#[derive(Debug)]
pub struct ResolvedVersion {
    pub version: String,
    pub source: String,
}

#[derive(Debug)]
pub struct ResolvedDependency {
    pub name: String,
    pub version: String,
}

#[derive(Debug)]
pub struct Resolved {
    pub dependencies: Vec<ResolvedDependency>,
    pub runtimes: HashMap<Runtime, ResolvedVersion>,
}

pub fn resolve(facts: Vec<Fact>) -> Resolved {
    let mut dependencies = Vec::new();
    let mut runtime_candidates: HashMap<Runtime, Vec<(String, String)>> = HashMap::new();

    for fact in facts {
        match fact {
            Fact::Dependency { name, version } => {
                dependencies.push(ResolvedDependency { name, version });
            }
            Fact::RuntimeVersion { runtime, version, source } => {
                runtime_candidates.entry(runtime).or_default().push((version, source));
            }
        }
    }

    let mut runtimes = HashMap::new();
    for (runtime, candidates) in runtime_candidates {
        let priority = source_priority(&runtime);
        // Pick the candidate whose source matches the earliest entry in the priority list.
        let best = candidates.into_iter()
            .min_by_key(|(_, source)| {
                priority.iter()
                    .position(|p| source.ends_with(p))
                    .unwrap_or(usize::MAX)
            });
        if let Some((version, source)) = best {
            runtimes.insert(runtime, ResolvedVersion { version, source });
        }
    }

    Resolved { dependencies, runtimes }
}
```

- [ ] **Step 5: Add integration test using full project fixture**

Add to `resolver.rs` tests:

```rust
    #[test]
    fn integration_full_project() {
        use crate::parsers::parse_directory;
        use std::path::PathBuf;

        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/resolver/full_project");
        let facts = parse_directory(&dir);
        let resolved = resolve(facts);

        assert_eq!(resolved.dependencies.len(), 1);
        assert_eq!(resolved.dependencies[0].name, "express");

        let node = resolved.runtimes.get(&Runtime::NodeJs).unwrap();
        assert_eq!(node.version, "20.11.0"); // .nvmrc wins over engines.node

        let python = resolved.runtimes.get(&Runtime::Python).unwrap();
        assert_eq!(python.version, "3.12.1");
    }
```

- [ ] **Step 6: Add `mod resolver;` to `main.rs`**

- [ ] **Step 7: Run all tests**

Run: `cargo test`
Expected: All pass.

- [ ] **Step 8: Run pre-commit checks and commit**

```bash
cargo fmt -- --check && cargo clippy -- -D warnings && cargo test
git add src/parsers/mod.rs src/resolver.rs tests/fixtures/resolver/
git commit -m "feat: file discovery and runtime priority resolver"
```

---

### Task 7: npm Registry Client

Async HTTP client for querying the npm registry.

**Files:**
- Create: `src/registries/mod.rs`
- Create: `src/registries/npm.rs`

- [ ] **Step 1: Write tests for npm response parsing**

The HTTP call itself will be tested via integration tests. Unit tests cover response parsing.

```rust
// src/registries/npm.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_npm_response_normal_package() {
        let json = serde_json::json!({
            "dist-tags": { "latest": "4.18.2" },
            "time": {
                "4.18.2": "2024-10-01T00:00:00.000Z"
            },
            "versions": {
                "4.18.2": {}
            }
        });
        let info = NpmPackageInfo::from_registry_response(&json).unwrap();
        assert_eq!(info.latest_version, "4.18.2");
        assert!(!info.deprecated);
        assert!(info.latest_publish_date.is_some());
    }

    #[test]
    fn parse_npm_response_deprecated() {
        let json = serde_json::json!({
            "dist-tags": { "latest": "2.88.2" },
            "time": { "2.88.2": "2020-03-15T00:00:00.000Z" },
            "versions": {
                "2.88.2": { "deprecated": "request has been deprecated" }
            }
        });
        let info = NpmPackageInfo::from_registry_response(&json).unwrap();
        assert!(info.deprecated);
    }

    #[test]
    fn parse_npm_response_missing_time() {
        let json = serde_json::json!({
            "dist-tags": { "latest": "1.0.0" },
            "versions": { "1.0.0": {} }
        });
        let info = NpmPackageInfo::from_registry_response(&json).unwrap();
        assert!(info.latest_publish_date.is_none());
    }
}
```

- [ ] **Step 2: Implement npm client**

```rust
// src/registries/npm.rs
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

const NPM_REGISTRY: &str = "https://registry.npmjs.org";
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct NpmPackageInfo {
    pub latest_version: String,
    pub deprecated: bool,
    pub latest_publish_date: Option<DateTime<Utc>>,
}

impl NpmPackageInfo {
    pub fn from_registry_response(json: &Value) -> Option<Self> {
        let latest = json.get("dist-tags")?.get("latest")?.as_str()?;

        let deprecated = json.get("versions")
            .and_then(|v| v.get(latest))
            .and_then(|v| v.get("deprecated"))
            .is_some();

        let publish_date = json.get("time")
            .and_then(|t| t.get(latest))
            .and_then(|d| d.as_str())
            .and_then(|d| d.parse::<DateTime<Utc>>().ok());

        Some(Self {
            latest_version: latest.to_string(),
            deprecated,
            latest_publish_date: publish_date,
        })
    }
}

pub async fn fetch_package(client: &Client, name: &str) -> Result<NpmPackageInfo, NpmError> {
    let url = format!("{NPM_REGISTRY}/{name}");
    let resp = client
        .get(&url)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| NpmError::Http(e.to_string()))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(NpmError::NotFound);
    }

    let json: Value = resp.json().await
        .map_err(|e| NpmError::Http(e.to_string()))?;

    NpmPackageInfo::from_registry_response(&json)
        .ok_or(NpmError::ParseError)
}

#[derive(Debug)]
pub enum NpmError {
    NotFound,
    Http(String),
    ParseError,
}
```

- [ ] **Step 3: Create `registries/mod.rs`**

```rust
// src/registries/mod.rs
pub mod npm;
pub mod eol;
```

- [ ] **Step 4: Add `mod registries;` to `main.rs`**

- [ ] **Step 5: Run tests**

Run: `cargo test`
Expected: All pass.

- [ ] **Step 6: Run pre-commit checks and commit**

```bash
cargo fmt -- --check && cargo clippy -- -D warnings && cargo test
git add src/registries/
git commit -m "feat: npm registry client with response parsing"
```

---

### Task 8: endoflife.date API Client

Query endoflife.date for runtime EOL cycles.

**Files:**
- Create: `src/registries/eol.rs`

- [ ] **Step 1: Write tests for EOL response parsing**

```rust
// src/registries/eol.rs
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn parse_cycle_with_date_eol() {
        let json = serde_json::json!([
            { "cycle": "20", "eol": "2026-04-30", "latest": "20.18.0" },
            { "cycle": "18", "eol": "2025-04-30", "latest": "18.20.4" }
        ]);
        let info = find_cycle(&json, "18").unwrap();
        assert_eq!(info.cycle, "18");
        assert_eq!(info.eol_date, Some(NaiveDate::from_ymd_opt(2025, 4, 30).unwrap()));
    }

    #[test]
    fn parse_cycle_with_false_eol() {
        let json = serde_json::json!([
            { "cycle": "22", "eol": false, "latest": "22.12.0" }
        ]);
        let info = find_cycle(&json, "22").unwrap();
        assert_eq!(info.eol_date, None);
    }

    #[test]
    fn cycle_not_found() {
        let json = serde_json::json!([
            { "cycle": "20", "eol": "2026-04-30" }
        ]);
        assert!(find_cycle(&json, "99").is_none());
    }

    #[test]
    fn normalize_version_nodejs() {
        assert_eq!(normalize_version(Runtime::NodeJs, "20.11.0"), "20");
        assert_eq!(normalize_version(Runtime::NodeJs, "v18.19.0"), "18");
    }

    #[test]
    fn normalize_version_python() {
        assert_eq!(normalize_version(Runtime::Python, "3.12.1"), "3.12");
        assert_eq!(normalize_version(Runtime::Python, "3.10"), "3.10");
    }

    #[test]
    fn normalize_version_java() {
        assert_eq!(normalize_version(Runtime::Java, "17"), "17");
        assert_eq!(normalize_version(Runtime::Java, "21.0.1"), "21");
    }
}
```

- [ ] **Step 2: Implement EOL client**

```rust
// src/registries/eol.rs
use chrono::NaiveDate;
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use crate::model::{EolInfo, Runtime};

const EOL_API: &str = "https://endoflife.date/api";
const TIMEOUT: Duration = Duration::from_secs(10);

pub fn normalize_version(runtime: Runtime, version: &str) -> String {
    let v = version.strip_prefix('v').unwrap_or(version);
    let parts: Vec<&str> = v.split('.').collect();
    match runtime {
        Runtime::NodeJs | Runtime::Java => parts[0].to_string(),
        Runtime::Python => {
            if parts.len() >= 2 {
                format!("{}.{}", parts[0], parts[1])
            } else {
                parts[0].to_string()
            }
        }
    }
}

#[derive(Debug)]
pub struct CycleInfo {
    pub cycle: String,
    pub eol_date: Option<NaiveDate>,
}

pub fn find_cycle(cycles: &Value, target_cycle: &str) -> Option<CycleInfo> {
    let arr = cycles.as_array()?;
    for entry in arr {
        let cycle = entry.get("cycle")?.as_str()?;
        if cycle == target_cycle {
            let eol_date = match entry.get("eol")? {
                Value::String(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d").ok(),
                Value::Bool(false) => None,
                _ => None,
            };
            return Some(CycleInfo {
                cycle: cycle.to_string(),
                eol_date,
            });
        }
    }
    None
}

pub async fn fetch_eol(client: &Client, runtime: Runtime, version: &str) -> Result<EolInfo, String> {
    let slug = runtime.eol_slug();
    let cycle = normalize_version(runtime, version);
    let url = format!("{EOL_API}/{slug}.json");

    let resp = client
        .get(&url)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let json: Value = resp.json().await.map_err(|e| e.to_string())?;

    let info = find_cycle(&json, &cycle)
        .ok_or_else(|| format!("cycle {cycle} not found for {slug}"))?;

    let days_left = info.eol_date.map(|eol| {
        let today = chrono::Utc::now().date_naive();
        (eol - today).num_days()
    });

    Ok(EolInfo {
        eol_date: info.eol_date,
        days_left,
        cycle: info.cycle,
        ref_url: format!("https://endoflife.date/{slug}"),
    })
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: All pass.

- [ ] **Step 4: Run pre-commit checks and commit**

```bash
cargo fmt -- --check && cargo clippy -- -D warnings && cargo test
git add src/registries/eol.rs
git commit -m "feat: endoflife.date API client with version normalization"
```

---

### Task 9: Signal Generation

Convert registry/EOL data into severity-tagged signals.

**Files:**
- Create: `src/signal.rs`

- [ ] **Step 1: Write tests**

```rust
// src/signal.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries::npm::NpmPackageInfo;
    use chrono::{NaiveDate, Utc, Duration};

    #[test]
    fn deprecated_package() {
        let info = NpmPackageInfo {
            latest_version: "2.88.2".into(),
            deprecated: true,
            latest_publish_date: None,
        };
        let signals = npm_signals("2.88.2", &info, 18);
        assert!(signals.iter().any(|s| s.kind == SignalKind::Deprecated));
        assert!(signals.iter().any(|s| s.severity == Severity::Critical));
    }

    #[test]
    fn behind_major() {
        let info = NpmPackageInfo {
            latest_version: "5.0.0".into(),
            deprecated: false,
            latest_publish_date: Some(Utc::now()),
        };
        let signals = npm_signals("4.18.2", &info, 18);
        assert!(signals.iter().any(|s| s.kind == SignalKind::BehindMajor));
    }

    #[test]
    fn behind_minor() {
        let info = NpmPackageInfo {
            latest_version: "4.19.0".into(),
            deprecated: false,
            latest_publish_date: Some(Utc::now()),
        };
        let signals = npm_signals("4.17.21", &info, 18);
        assert!(signals.iter().any(|s| s.kind == SignalKind::BehindMinor));
        assert!(!signals.iter().any(|s| s.kind == SignalKind::BehindMajor));
    }

    #[test]
    fn stale_package() {
        let info = NpmPackageInfo {
            latest_version: "1.0.0".into(),
            deprecated: false,
            latest_publish_date: Some(Utc::now() - Duration::days(600)),
        };
        let signals = npm_signals("1.0.0", &info, 18);
        assert!(signals.iter().any(|s| s.kind == SignalKind::Stale));
    }

    #[test]
    fn up_to_date_package() {
        let info = NpmPackageInfo {
            latest_version: "4.17.21".into(),
            deprecated: false,
            latest_publish_date: Some(Utc::now()),
        };
        let signals = npm_signals("4.17.21", &info, 18);
        assert!(signals.is_empty());
    }

    #[test]
    fn eol_runtime() {
        let eol_info = EolInfo {
            eol_date: Some(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
            days_left: Some(-400),
            cycle: "16".into(),
            ref_url: "https://endoflife.date/nodejs".into(),
        };
        let signals = eol_signals(&eol_info, 180);
        assert!(signals.iter().any(|s| s.kind == SignalKind::Eol));
    }

    #[test]
    fn approaching_eol() {
        let eol_info = EolInfo {
            eol_date: Some(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()),
            days_left: Some(74),
            cycle: "18".into(),
            ref_url: "https://endoflife.date/nodejs".into(),
        };
        let signals = eol_signals(&eol_info, 180);
        assert!(signals.iter().any(|s| s.kind == SignalKind::ApproachingEol));
    }

    #[test]
    fn no_eol_date_no_signal() {
        let eol_info = EolInfo {
            eol_date: None,
            days_left: None,
            cycle: "22".into(),
            ref_url: "https://endoflife.date/nodejs".into(),
        };
        let signals = eol_signals(&eol_info, 180);
        assert!(signals.is_empty());
    }
}
```

- [ ] **Step 2: Implement signal generation**

```rust
// src/signal.rs
use crate::model::{EolInfo, Signal, SignalKind, Severity};
use crate::registries::npm::NpmPackageInfo;

pub fn npm_signals(installed_version: &str, info: &NpmPackageInfo, stale_months: u32) -> Vec<Signal> {
    let mut signals = Vec::new();

    if info.deprecated {
        signals.push(Signal {
            kind: SignalKind::Deprecated,
            severity: Severity::Critical,
            message: "package is deprecated".into(),
        });
    }

    // Version comparison
    if let (Ok(installed), Ok(latest)) = (
        semver::Version::parse(installed_version),
        semver::Version::parse(&info.latest_version),
    ) {
        if latest.major > installed.major {
            signals.push(Signal {
                kind: SignalKind::BehindMajor,
                severity: Severity::Warning,
                message: format!("{} major version(s) behind", latest.major - installed.major),
            });
        } else if latest.minor > installed.minor {
            signals.push(Signal {
                kind: SignalKind::BehindMinor,
                severity: Severity::Warning,
                message: format!("{} minor version(s) behind", latest.minor - installed.minor),
            });
        }
    }

    // Staleness
    if let Some(publish_date) = info.latest_publish_date {
        let months_old = (chrono::Utc::now() - publish_date).num_days() / 30;
        if months_old > stale_months as i64 {
            signals.push(Signal {
                kind: SignalKind::Stale,
                severity: Severity::Warning,
                message: format!("latest version published {months_old} months ago"),
            });
        }
    }

    signals
}

pub fn eol_signals(eol_info: &EolInfo, threshold_days: u32) -> Vec<Signal> {
    let mut signals = Vec::new();

    if let Some(days_left) = eol_info.days_left {
        if days_left < 0 {
            signals.push(Signal {
                kind: SignalKind::Eol,
                severity: Severity::Critical,
                message: format!("EOL {} (expired {} days ago)", eol_info.eol_date.unwrap(), -days_left),
            });
        } else if days_left < threshold_days as i64 {
            signals.push(Signal {
                kind: SignalKind::ApproachingEol,
                severity: Severity::Warning,
                message: format!("EOL {} ({days_left} days left)", eol_info.eol_date.unwrap()),
            });
        }
    }

    signals
}
```

- [ ] **Step 3: Add `mod signal;` to `main.rs`**

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: All pass.

- [ ] **Step 5: Run pre-commit checks and commit**

```bash
cargo fmt -- --check && cargo clippy -- -D warnings && cargo test
git add src/signal.rs
git commit -m "feat: signal generation for npm packages and runtime EOL"
```

---

### Task 10: Output Formatting

Terminal table rendering and JSON output.

**Files:**
- Create: `src/output.rs`

- [ ] **Step 1: Write tests**

```rust
// src/output.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use chrono::Utc;
    use std::path::PathBuf;

    fn sample_result() -> ScanResult {
        ScanResult {
            findings: vec![
                Finding {
                    ecosystem: Ecosystem::Runtime,
                    name: "Node.js".into(),
                    installed_version: "18.19.0".into(),
                    latest_version: None,
                    signals: vec![Signal {
                        kind: SignalKind::Eol,
                        severity: Severity::Critical,
                        message: "EOL 2025-04-30".into(),
                    }],
                    eol_info: Some(EolInfo {
                        eol_date: Some(chrono::NaiveDate::from_ymd_opt(2025, 4, 30).unwrap()),
                        days_left: Some(-324),
                        cycle: "18".into(),
                        ref_url: "https://endoflife.date/nodejs".into(),
                    }),
                },
                Finding {
                    ecosystem: Ecosystem::Npm,
                    name: "lodash".into(),
                    installed_version: "4.17.21".into(),
                    latest_version: Some("4.17.21".into()),
                    signals: vec![],
                    eol_info: None,
                },
            ],
            counts: Counts { total: 2, critical: 1, warning: 0, ok: 1 },
            scanned_at: Utc::now(),
            path: PathBuf::from("."),
        }
    }

    #[test]
    fn json_output_is_valid_json() {
        let result = sample_result();
        let json = format_json(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("findings").unwrap().is_array());
    }

    #[test]
    fn terminal_output_contains_summary() {
        let result = sample_result();
        let output = format_terminal(&result);
        assert!(output.contains("Summary:"));
        assert!(output.contains("1 critical"));
    }
}
```

- [ ] **Step 2: Implement output formatting**

```rust
// src/output.rs
use crate::model::{Ecosystem, ScanResult, Severity};

pub fn format_json(result: &ScanResult) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(result)
}

pub fn format_terminal(result: &ScanResult) -> String {
    use owo_colors::OwoColorize;

    let mut out = String::new();

    out.push_str(&format!("shelflife v{} — scanning {}\n\n",
        env!("CARGO_PKG_VERSION"),
        result.path.display()));

    // Runtime findings
    let runtimes: Vec<_> = result.findings.iter()
        .filter(|f| f.ecosystem == Ecosystem::Runtime)
        .collect();
    if !runtimes.is_empty() {
        out.push_str("Runtime EOL\n");
        for f in &runtimes {
            let status = finding_status(f);
            let msg = f.signals.first().map(|s| s.message.as_str()).unwrap_or("OK");
            out.push_str(&format!("  {} {}    {}\n",
                format!("{} {}", f.name, f.installed_version),
                msg,
                status));
        }
        out.push('\n');
    }

    // npm findings
    let npm: Vec<_> = result.findings.iter()
        .filter(|f| f.ecosystem == Ecosystem::Npm)
        .collect();
    if !npm.is_empty() {
        out.push_str(&format!("npm Dependencies ({} packages)\n", npm.len()));
        for f in &npm {
            let status = finding_status(f);
            let detail = if let Some(latest) = &f.latest_version {
                if let Some(signal) = f.signals.first() {
                    format!("latest {}  ({})", latest, signal.message)
                } else {
                    format!("latest {}", latest)
                }
            } else {
                String::new()
            };
            out.push_str(&format!("  {} {}    {}\n",
                format!("{} {}", f.name, f.installed_version),
                detail,
                status));
        }
        out.push('\n');
    }

    out.push_str(&format!("Summary: {} checked, {} critical, {} warning, {} ok\n",
        result.counts.total, result.counts.critical, result.counts.warning, result.counts.ok));

    out
}

fn finding_status(f: &crate::model::Finding) -> String {
    use owo_colors::OwoColorize;

    let max_severity = f.signals.iter()
        .map(|s| s.severity)
        .max()
        .unwrap_or(Severity::Info);

    match max_severity {
        Severity::Critical => "[CRITICAL]".red().bold().to_string(),
        Severity::Warning => "[WARNING]".yellow().to_string(),
        Severity::Info => "[OK]".green().to_string(),
    }
}

- [ ] **Step 3: Add `mod output;` to `main.rs`**

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: All pass.

- [ ] **Step 5: Run pre-commit checks and commit**

```bash
cargo fmt -- --check && cargo clippy -- -D warnings && cargo test
git add src/output.rs
git commit -m "feat: terminal table and JSON output formatting"
```

---

### Task 11: Main Orchestration

Wire everything together in `main.rs` — the full scan flow.

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Implement the async scan orchestration**

Replace the body of `main()` (after config loading) with:

```rust
#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Config loading (from Task 2 — already done)
    let config_path = cli.config.clone()
        .unwrap_or_else(|| cli.path.join(".shelflife.toml"));
    let file_config = if config_path.exists() {
        Config::from_toml(&config_path).unwrap_or_else(|e| {
            eprintln!("warning: failed to read config {}: {e}", config_path.display());
            Config::default()
        })
    } else {
        Config::default()
    };
    let overrides = CliOverrides {
        threshold_days: cli.threshold_days,
        stale_months: cli.stale_months,
        ignore: cli.ignore,
        fail_on: cli.fail_on,
        json: cli.json,
        verbose: cli.verbose,
    };
    let mut config = file_config.merge(overrides);
    config.path = cli.path;

    // Parse + resolve
    let facts = parsers::parse_directory(&config.path);
    let resolved = resolver::resolve(facts);

    if config.verbose {
        for (runtime, rv) in &resolved.runtimes {
            eprintln!("runtime {:?}: {} (from {})", runtime, rv.version, rv.source);
        }
        if resolved.dependencies.is_empty() {
            eprintln!("no package-lock.json found, skipping npm dependency analysis");
        }
    }

    let http_client = reqwest::Client::new();
    let mut findings = Vec::new();

    // Runtime EOL checks
    for (runtime, rv) in &resolved.runtimes {
        let display_name = match runtime {
            model::Runtime::NodeJs => "Node.js",
            model::Runtime::Python => "Python",
            model::Runtime::Java => "Java",
        };
        match registries::eol::fetch_eol(&http_client, *runtime, &rv.version).await {
            Ok(eol_info) => {
                let signals = signal::eol_signals(&eol_info, config.threshold_days);
                findings.push(model::Finding {
                    ecosystem: model::Ecosystem::Runtime,
                    name: display_name.to_string(),
                    installed_version: rv.version.clone(),
                    latest_version: None,
                    signals,
                    eol_info: Some(eol_info),
                });
            }
            Err(e) => {
                if config.verbose {
                    eprintln!("warning: EOL check for {} failed: {e}", display_name);
                }
                findings.push(model::Finding {
                    ecosystem: model::Ecosystem::Runtime,
                    name: display_name.to_string(),
                    installed_version: rv.version.clone(),
                    latest_version: None,
                    signals: vec![model::Signal {
                        kind: model::SignalKind::RegistryError,
                        severity: model::Severity::Info,
                        message: format!("EOL check failed: {e}"),
                    }],
                    eol_info: None,
                });
            }
        }
    }

    // npm dependency checks (concurrent, max 8 in-flight)
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(8));
    let mut handles = Vec::new();

    for dep in &resolved.dependencies {
        if config.ignore.contains(&dep.name) {
            continue;
        }
        let client = http_client.clone();
        let name = dep.name.clone();
        let version = dep.version.clone();
        let stale_months = config.stale_months;
        let permit = semaphore.clone();

        handles.push(tokio::spawn(async move {
            let _permit = permit.acquire().await.unwrap();
            let result = registries::npm::fetch_package(&client, &name).await;
            (name, version, stale_months, result)
        }));
    }

    for handle in handles {
        let (name, version, stale_months, result) = handle.await.unwrap();
        match result {
            Ok(info) => {
                let signals = signal::npm_signals(&version, &info, stale_months);
                findings.push(model::Finding {
                    ecosystem: model::Ecosystem::Npm,
                    name,
                    installed_version: version,
                    latest_version: Some(info.latest_version),
                    signals,
                    eol_info: None,
                });
            }
            Err(registries::npm::NpmError::NotFound) => {
                findings.push(model::Finding {
                    ecosystem: model::Ecosystem::Npm,
                    name,
                    installed_version: version,
                    latest_version: None,
                    signals: vec![model::Signal {
                        kind: model::SignalKind::NotFound,
                        severity: model::Severity::Info,
                        message: "package not found in registry".into(),
                    }],
                    eol_info: None,
                });
            }
            Err(e) => {
                findings.push(model::Finding {
                    ecosystem: model::Ecosystem::Npm,
                    name,
                    installed_version: version,
                    latest_version: None,
                    signals: vec![model::Signal {
                        kind: model::SignalKind::RegistryError,
                        severity: model::Severity::Info,
                        message: format!("{e:?}"),
                    }],
                    eol_info: None,
                });
            }
        }
    }

    // Count
    let counts = model::Counts {
        total: findings.len(),
        critical: findings.iter().filter(|f| f.signals.iter().any(|s| s.severity == model::Severity::Critical)).count(),
        warning: findings.iter().filter(|f| {
            !f.signals.iter().any(|s| s.severity == model::Severity::Critical) &&
            f.signals.iter().any(|s| s.severity == model::Severity::Warning)
        }).count(),
        ok: findings.iter().filter(|f| f.signals.iter().all(|s| s.severity <= model::Severity::Info)).count(),
    };

    let result = model::ScanResult {
        findings,
        counts,
        scanned_at: chrono::Utc::now(),
        path: config.path.clone(),
    };

    // Output
    if config.json {
        println!("{}", output::format_json(&result).unwrap());
    } else {
        print!("{}", output::format_terminal(&result));
    }

    // Exit code
    let should_fail = match config.fail_on {
        model::FailOn::Any => result.counts.critical > 0 || result.counts.warning > 0,
        model::FailOn::Critical => result.counts.critical > 0,
        model::FailOn::None => false,
    };
    if should_fail {
        std::process::exit(1);
    }
}
```

- [ ] **Step 2: Run pre-commit checks**

Run: `cargo fmt -- --check && cargo clippy -- -D warnings && cargo test`
Expected: All pass. Fix any compilation issues from wiring.

- [ ] **Step 3: Manual smoke test**

Run against the shelf-life-github-app project (has package-lock.json + .nvmrc):

```bash
cargo run -- /Users/rroskam/repos/shelf-life-github-app
```

Expected: Terminal output with runtime EOL status and npm dependency findings.

- [ ] **Step 4: Commit**

```bash
cargo fmt -- --check && cargo clippy -- -D warnings && cargo test
git add src/main.rs
git commit -m "feat: wire up full scan orchestration in main"
```

---

### Task 12: Exit Code + JSON Integration Tests

End-to-end tests verifying exit codes and JSON output.

**Files:**
- Create: `tests/integration.rs`
- Create: `tests/fixtures/integration/clean_project/` (project with all-ok deps)

- [ ] **Step 1: Create test fixture — minimal project**

`tests/fixtures/integration/clean_project/package-lock.json`:
```json
{
  "name": "clean-test",
  "lockfileVersion": 3,
  "packages": {
    "": {}
  }
}
```

`tests/fixtures/integration/clean_project/.nvmrc`: `22`

- [ ] **Step 2: Write integration tests**

```rust
// tests/integration.rs
use std::process::Command;
use std::path::PathBuf;

fn shelflife() -> Command {
    Command::new(env!("CARGO_BIN_EXE_shelflife"))
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/integration")
        .join(name)
}

#[test]
fn json_output_is_valid() {
    let output = shelflife()
        .arg(fixture("clean_project"))
        .arg("--json")
        .arg("--fail-on")
        .arg("none")
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(parsed.get("findings").is_some());
    assert!(parsed.get("counts").is_some());
}

#[test]
fn fail_on_none_always_exits_zero() {
    let status = shelflife()
        .arg(fixture("clean_project"))
        .arg("--fail-on")
        .arg("none")
        .status()
        .unwrap();

    assert!(status.success());
}

#[test]
fn help_flag_works() {
    let output = shelflife()
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("shelflife"));
}

#[test]
fn version_flag_works() {
    let output = shelflife()
        .arg("--version")
        .output()
        .unwrap();

    assert!(output.status.success());
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --test integration`
Expected: All pass.

- [ ] **Step 4: Run pre-commit checks and commit**

```bash
cargo fmt -- --check && cargo clippy -- -D warnings && cargo test
git add tests/
git commit -m "test: integration tests for CLI exit codes and JSON output"
```

---

### Task 13: Final Smoke Test

Verify the full test suite and run against a real project.

- [ ] **Step 1: Run the full pre-commit suite one last time**

```bash
cargo fmt -- --check && cargo clippy -- -D warnings && cargo test
```

Expected: All pass, no warnings.

- [ ] **Step 3: Run a real-world smoke test**

```bash
cargo run -- /Users/rroskam/repos/shelf-life-github-app --verbose
cargo run -- /Users/rroskam/repos/shelf-life-github-app --json | jq '.counts'
```

- [ ] **Step 4: Verify commit history**

```bash
git log --oneline
```

Expected commit log:
```
test: integration tests for CLI exit codes and JSON output
feat: wire up full scan orchestration in main
feat: terminal table and JSON output formatting
feat: signal generation for npm packages and runtime EOL
feat: endoflife.date API client with version normalization
feat: npm registry client with response parsing
feat: file discovery and runtime priority resolver
feat: Python and Java runtime parsers
feat: Node.js runtime parsers (.nvmrc, .node-version, engines.node)
feat: package-lock.json parser (v1/v2/v3)
feat: config loading with TOML + CLI override merging
feat: scaffold project with core model types
docs: simplify spec
docs: address spec review feedback
docs: add shelflife CLI design spec
```
