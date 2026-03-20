# shelflife — CLI Design Spec

## Overview

A Rust CLI tool that scans a local project directory for dependency and runtime end-of-life (EOL) risk. It checks npm packages against the npm registry and runtimes (Node.js, Python, Java, OS) against the endoflife.date API. Outputs a colored summary table to the terminal or structured JSON.

This is a Rust port of the core scanning logic from [shelf-life-github-app](../../../shelf-life-github-app), scoped to a standalone CLI with npm + runtime EOL support.

## CLI Interface

```
shelflife [OPTIONS] [PATH]

Arguments:
  [PATH]  Directory to scan (default: .)

Options:
  -c, --config <FILE>        Config file path (default: .shelflife.toml)
      --threshold-days <N>   Days before EOL to warn (default: 180)
      --stale-months <N>     Months without update = stale (default: 18)
      --ignore <PKG,...>     Packages to skip
      --json                 Output JSON instead of table
      --fail-on <LEVEL>      Exit 1 on: any (default), critical, none
  -v, --verbose              Verbose output
  -h, --help
  -V, --version
```

### Examples

```sh
# Scan current directory with defaults
shelflife

# Scan a specific project, output JSON
shelflife ~/projects/my-app --json

# Only fail on critical issues (EOL/deprecated), ignore warnings
shelflife --fail-on critical

# Ignore specific packages
shelflife --ignore lodash,moment
```

## Core Flow

1. **Load config** — CLI flags override `.shelflife.toml`, which overrides defaults
2. **Detect ecosystems** — scan target directory for `package.json`, `package-lock.json`, `Dockerfile`, `.nvmrc`, `.python-version`, `pom.xml`, etc.
3. **Resolve npm dependencies** — parse `package-lock.json` for top-level dependencies with pinned versions
4. **Lookup npm registry** — `GET https://registry.npmjs.org/{pkg}` for each dependency (concurrent, bounded to ~8 in-flight)
5. **Detect runtimes** — parse version from config files (`.nvmrc`, `engines.node` in `package.json`, `Dockerfile` FROM lines, `.python-version`, `pom.xml` source/target)
6. **Check runtime EOL** — `GET https://endoflife.date/api/{product}.json`, match version to cycle, calculate days until EOL
7. **Generate signals** — classify each finding: deprecated, stale, behind-major, behind-minor, EOL, approaching-EOL
8. **Output** — render colored terminal table (default) or JSON (`--json`)
9. **Exit** — code 0 if no findings at configured `--fail-on` level, code 1 otherwise

## Data Model

### ScanResult

The top-level output of a scan.

```
ScanResult {
    findings: Vec<Finding>,
    counts: Counts { total, critical, warning, ok },
    scanned_at: DateTime<Utc>,
    path: PathBuf,
}
```

### Finding

A single dependency or runtime that was checked.

```
Finding {
    ecosystem: Ecosystem (Npm | Runtime),
    name: String,
    installed_version: String,
    latest_version: Option<String>,
    signals: Vec<Signal>,
    eol_info: Option<EolInfo>,
}
```

### Signal

A risk indicator attached to a finding.

```
Signal {
    kind: SignalKind (Deprecated | Stale | BehindMajor | BehindMinor | Eol | ApproachingEol | RegistryError | NotFound),
    severity: Severity (Critical | Warning | Info),
    message: String,
}
```

Severity mapping:
- **Critical**: `Deprecated`, `Eol`
- **Warning**: `Stale`, `BehindMajor`, `BehindMinor`, `ApproachingEol`
- **Info**: `RegistryError`, `NotFound`

### EolInfo

Runtime-specific EOL data.

```
EolInfo {
    eol_date: NaiveDate,
    days_left: i64,      // negative = past EOL
    cycle: String,       // e.g. "18" for Node 18
    ref_url: String,     // link to endoflife.date
}
```

### Config

Merged from defaults + `.shelflife.toml` + CLI flags.

```
Config {
    threshold_days: u32,       // default: 180
    stale_months: u32,         // default: 18
    ignore: Vec<String>,       // default: []
    fail_on: FailOn,           // default: Any
    json: bool,                // default: false
    verbose: bool,             // default: false
    path: PathBuf,             // default: .
}
```

## Ecosystem Detection

### npm

Detected by presence of `package.json`. Dependencies resolved from:
1. `package-lock.json` (preferred — has pinned versions)
2. `package.json` `dependencies` + `devDependencies` (fallback — may have ranges)

### Runtime Detection

| Runtime | Source files | Version extraction |
|---------|------------|-------------------|
| Node.js | `.nvmrc`, `.node-version`, `package.json` `engines.node` | Parse version string, strip `v` prefix, semver coerce |
| Python  | `.python-version`, `runtime.txt`, `pyproject.toml` `requires-python` | Parse version string |
| Java    | `pom.xml` `<source>`/`<target>`/`<release>`, `build.gradle` `sourceCompatibility` | Parse major version |
| Ubuntu  | `Dockerfile` `FROM ubuntu:XX.XX` | Parse tag |
| Alpine  | `Dockerfile` `FROM alpine:X.XX` | Parse tag |
| Debian  | `Dockerfile` `FROM debian:XX` / `debian:codename` | Parse tag or codename |

## Registry & API Integration

### npm Registry

- Endpoint: `GET https://registry.npmjs.org/{package}`
- Extract: `dist-tags.latest`, `time.{version}` (publish dates), `versions.{v}.deprecated`
- Concurrency: max 8 in-flight requests
- Timeout: 10s per request

### endoflife.date API

- Endpoint: `GET https://endoflife.date/api/{product}.json`
- Products: `nodejs`, `python`, `java`, `ubuntu`, `alpine`, `debian`
- Match detected version to a cycle, read `eol` date field
- Timeout: 10s per request

## Signal Generation

For each npm package:
- **Deprecated**: registry metadata has deprecation message → `Critical`
- **Stale**: latest version published > `stale_months` ago → `Warning`
- **BehindMajor**: installed major < latest major → `Warning`
- **BehindMinor**: installed minor < latest minor (same major) → `Warning`
- **NotFound**: package not in registry → `Info`
- **RegistryError**: HTTP error fetching metadata → `Info`

For each runtime:
- **Eol**: `days_left < 0` → `Critical`
- **ApproachingEol**: `0 < days_left < threshold_days` → `Warning`

## Output Formats

### Terminal (default)

Colored table with emoji status indicators:

```
shelflife v0.1.0 — scanning /Users/you/project

Runtime EOL
  Node.js 18.19.0    EOL 2025-04-30 (expired 324 days ago)     [CRITICAL]
  Python 3.9.18      EOL 2025-10-05 (200 days left)            [WARNING]

npm Dependencies (47 packages)
  lodash 4.17.21     latest 4.17.21                             [OK]
  express 4.18.2     latest 5.1.0  (1 major behind)            [WARNING]
  request 2.88.2     DEPRECATED                                 [CRITICAL]

Summary: 47 checked, 2 critical, 1 warning, 44 ok
```

### JSON (`--json`)

Outputs the `ScanResult` struct as JSON to stdout. Suitable for piping to `jq` or other tools.

## Configuration File

`.shelflife.toml` in the scanned directory (or specified via `--config`):

```toml
threshold_days = 180
stale_months = 18
ignore = ["some-internal-pkg"]
fail_on = "any"  # any | critical | none
```

Precedence: CLI flags > config file > defaults.

## Exit Codes

- `0` — no findings at the configured `fail_on` level
- `1` — findings found at or above the configured level

With `--fail-on any` (default): any finding triggers exit 1.
With `--fail-on critical`: only critical findings (deprecated, past EOL) trigger exit 1.
With `--fail-on none`: always exits 0.

## Project Structure

```
shelflife/
  Cargo.toml
  src/
    main.rs              # Entry point, CLI parsing, orchestration
    config.rs            # Config loading (defaults + TOML + CLI merge)
    detect.rs            # Ecosystem and runtime detection
    npm/
      mod.rs             # npm dependency resolution
      registry.rs        # npm registry HTTP client
    eol.rs               # endoflife.date API client
    signal.rs            # Signal generation logic
    model.rs             # Core data types (Finding, Signal, ScanResult, etc.)
    output.rs            # Terminal table and JSON formatters
```

## Key Crates

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing with derive macros |
| `reqwest` | Async HTTP client |
| `tokio` | Async runtime |
| `semver` | Version parsing and comparison |
| `serde` + `serde_json` | JSON serialization/deserialization |
| `toml` | Config file parsing |
| `tabled` | Terminal table rendering |
| `owo-colors` | Terminal color output |
| `chrono` | Date handling and arithmetic |

## Out of Scope

These are explicitly excluded from the initial version and may be added later:

- **PyPI / Maven / Gradle ecosystem resolution** — runtimes are detected but package registries for these ecosystems are not queried
- **Report file generation** — no Markdown, CSV, or HTML output files
- **Audit trails** — no scan history or SOC2 logging
- **GitHub App integration** — this is a local CLI only
- **HTTP caching / rate limiting** — rely on connection pooling and concurrency limits
- **Lock file generation** — read-only tool, does not modify the project
