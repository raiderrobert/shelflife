# shelflife — CLI Design Spec

## Overview

A Rust CLI tool that scans a local project directory for dependency and runtime end-of-life (EOL) risk. It checks npm packages against the npm registry and runtimes (Node.js, Python, Java) against the endoflife.date API. Outputs a colored summary table to the terminal or structured JSON.

Inspired by [shelf-life-github-app](../../../shelf-life-github-app), scoped to a standalone CLI with npm + runtime EOL support.

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

## Architecture: File Parsers → Facts → Resolver

Detection is split into three layers. Each layer has a single responsibility, and new ecosystems or file types are added by writing a new parser — no changes to the resolver, signals, or output.

### Layer 1: File Parsers

Each parser reads one file type and emits a `Vec<Fact>`. Facts are typed observations with no interpretation.

```
enum Fact {
    /// Pinned dependency from a lockfile
    Dependency { name: String, version: String },

    /// Concrete runtime version (e.g., "20.11.0" from .nvmrc)
    RuntimeVersion { runtime: Runtime, version: String, source: String },
}

enum Runtime { NodeJs, Python, Java }
```

### Layer 2: File Parser Inventory

#### `package-lock.json` → `Dependency` facts

Parses npm lockfile for top-level dependencies with pinned versions. Lockfile is required for dependency checking — if absent, skip npm dependency analysis with a warning.

- **v2/v3 format (npm 7+):** Read the `packages` map. Dependencies are keyed as `node_modules/{name}`. Extract `version` field from each top-level entry. This is the primary format.
- **v1 format (npm 6):** Fall back to the `dependencies` map at the top level.
- **Detection:** Check `"lockfileVersion"` field. v2 lockfiles contain both `packages` and `dependencies` — prefer `packages`.

#### `package.json` → `RuntimeVersion` facts

- Read `engines.node` if present. Extract the minimum major version from the semver range (e.g., `">=18"` → `18`, `"^20.0.0"` → `20`).
- Emit `Fact::RuntimeVersion { runtime: NodeJs }`.
- This is a lowest-priority source for Node.js version — used only if `.nvmrc` / `.node-version` are absent.

#### `.nvmrc` / `.node-version` → `RuntimeVersion` facts

- Trim whitespace, strip `v` prefix if present.
- If the value is numeric (e.g., `20`, `20.11.0`): emit `Fact::RuntimeVersion { runtime: NodeJs }`.
- If the value is an alias (`lts/*`, `lts/iron`, `node`, `stable`): log a warning with `--verbose` that aliases are not supported, skip.

#### `.python-version` / `runtime.txt` → `RuntimeVersion` facts

- `.python-version`: single version string, e.g., `3.12.1` → emit `Fact::RuntimeVersion { runtime: Python }`.
- `runtime.txt`: format `python-3.12.1` → strip `python-` prefix.

#### `pyproject.toml` → `RuntimeVersion` facts

- Read `requires-python` from `[project]` table using a TOML parser.
- Extract minimum major.minor version from the specifier (e.g., `">=3.10"` → `3.10`).
- Lower priority than `.python-version` — used only if `.python-version` / `runtime.txt` are absent.

#### `pom.xml` → `RuntimeVersion` facts

- Read `<maven.compiler.source>`, `<maven.compiler.target>`, or `<maven.compiler.release>` from `<properties>`.
- Also check `<source>`, `<target>`, `<release>` inside `<maven-compiler-plugin>` configuration.
- Literal values only — property references like `${java.version}` are not resolved (log with `--verbose`).

#### `build.gradle` → `RuntimeVersion` facts

- Match `sourceCompatibility = '17'` or `sourceCompatibility = JavaVersion.VERSION_17` via regex.
- Literal values only — variable references are not resolved.

### Layer 3: Resolver

The resolver is simple: collect facts, deduplicate, pass to signal generation.

#### Dependencies

Use all `Dependency` facts from the lockfile parser. No deduplication needed — the lockfile has one entry per package.

#### Runtimes

For each runtime, if multiple `RuntimeVersion` facts exist, pick the first match from this priority order:

| Runtime | Priority (first found wins) |
|---------|----------------------------|
| Node.js | `.nvmrc` → `.node-version` → `package.json` `engines.node` |
| Python  | `.python-version` → `runtime.txt` → `pyproject.toml` |
| Java    | `pom.xml` → `build.gradle` |

No conflict resolution — first match wins. With `--verbose`, log which source was used.

#### Version Normalization

Normalize detected versions before matching against endoflife.date cycles:

- Strip `v` prefix
- Node.js and Java: use major version (e.g., `20.11.0` → cycle `20`)
- Python: use major.minor (e.g., `3.12.1` → cycle `3.12`)

## Data Model

### ScanResult

```
ScanResult {
    findings: Vec<Finding>,
    counts: Counts { total, critical, warning, ok },
    scanned_at: DateTime<Utc>,
    path: PathBuf,
}
```

### Finding

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

```
EolInfo {
    eol_date: Option<NaiveDate>,  // None when endoflife.date returns eol: false
    days_left: Option<i64>,       // None when no EOL date; negative = past EOL
    cycle: String,                // e.g. "20" for Node 20
    ref_url: String,              // link to endoflife.date
}
```

### Config

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

## Core Flow

1. **Load config** — CLI flags override `.shelflife.toml`, which overrides defaults
2. **Discover files** — scan target directory root (no recursion) for known file names
3. **Parse files → facts** — run each discovered file through its parser, collect all facts
4. **Resolve** — pick best runtime source per priority, collect dependencies
5. **Lookup npm registry** — `GET https://registry.npmjs.org/{pkg}` for each dependency (concurrent, max 8 in-flight)
6. **Check runtime EOL** — `GET https://endoflife.date/api/{product}.json` for each resolved runtime
7. **Generate signals** — classify each finding
8. **Output** — colored terminal table or JSON (`--json`)
9. **Exit** — code 0 or 1 based on `--fail-on` level

## Registry & API Integration

### npm Registry

- Endpoint: `GET https://registry.npmjs.org/{package}`
- Extract: `dist-tags.latest`, `time.{version}` (publish dates), `versions.{v}.deprecated`
- Concurrency: max 8 in-flight requests
- Timeout: 10s per request
- No retries; failed lookups produce an Info-level `RegistryError` signal

### endoflife.date API

- Endpoint: `GET https://endoflife.date/api/{product}.json`
- **Product slug mapping:**

  | Runtime | API slug |
  |---------|----------|
  | Node.js | `nodejs` |
  | Python  | `python` |
  | Java    | `java`   |

- `eol` field can be `false` (boolean, not yet EOL) or a date string — handle both
- No retries; failed lookups produce an Info-level signal
- Timeout: 10s per request

## Signal Generation

For each npm package:
- **Deprecated**: registry metadata has deprecation message → `Critical`
- **Stale**: `dist-tags.latest` publish date older than `stale_months` → `Warning`
- **BehindMajor**: installed major < latest major → `Warning`
- **BehindMinor**: installed minor < latest minor (same major) → `Warning`
- **NotFound**: package not in registry → `Info`
- **RegistryError**: HTTP error fetching metadata → `Info`

For each runtime:
- **Eol**: `days_left < 0` → `Critical`
- **ApproachingEol**: `0 < days_left < threshold_days` → `Warning`

## Output

### Terminal (default)

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

Outputs `ScanResult` as JSON to stdout.

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
- `1` — findings at or above the configured level

`--fail-on any` (default): Warning or Critical triggers exit 1.
`--fail-on critical`: only Critical triggers exit 1.
`--fail-on none`: always exits 0.

## Project Structure

```
shelflife/
  Cargo.toml
  src/
    main.rs              # Entry point, CLI parsing, orchestration
    config.rs            # Config loading (defaults + TOML + CLI merge)
    model.rs             # Core data types (Fact, Finding, Signal, ScanResult, etc.)
    parsers/
      mod.rs             # File discovery + parser dispatch
      lockfile.rs        # package-lock.json parser (v1/v2/v3)
      package_json.rs    # package.json engines.node parser
      nvmrc.rs           # .nvmrc / .node-version parser
      python.rs          # .python-version, runtime.txt, pyproject.toml
      java.rs            # pom.xml, build.gradle
    resolver.rs          # Fact collection + runtime priority resolution
    registries/
      mod.rs             # Shared HTTP client, concurrency control
      npm.rs             # npm registry client
      eol.rs             # endoflife.date API client
    signal.rs            # Signal generation logic
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

## Out of Scope (v1)

These are deferred, not rejected. The parser → fact → resolver architecture is designed so most of these are "add a new parser" or "add a new registry client."

- **Dockerfile parsing** — and with it OS runtime EOL (Ubuntu, Alpine, Debian). Requires ARG expansion and multi-stage handling. Add as a `parsers/dockerfile.rs` later.
- **`.nvmrc` alias resolution** (`lts/*`, `lts/iron`) — requires a secondary API call to nodejs.org. Add as a resolver enhancement later.
- **PyPI / Maven / Gradle registry lookups** — add as new `registries/` modules later.
- **pnpm / yarn lockfile support** — add as new `parsers/` modules later. Without a lockfile, npm deps are skipped entirely.
- **Monorepo / workspace scanning** — only scans the target directory root.
- **`npm ls` invocation** — lockfile parsing only, no subprocess calls.
- **Report file generation** — no Markdown, CSV, or HTML files.
- **HTTP caching / retries** — rely on connection pooling and concurrency limits.
- **Local EOL database** — queries endoflife.date API directly.
