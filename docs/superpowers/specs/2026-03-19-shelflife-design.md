# shelflife — CLI Design Spec

## Overview

A Rust CLI tool that scans a local project directory for dependency and runtime end-of-life (EOL) risk. It checks npm packages against the npm registry and runtimes (Node.js, Python, Java, OS) against the endoflife.date API. Outputs a colored summary table to the terminal or structured JSON.

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

The detection system is split into three layers. This design is informed by how Renovate (manager → datasource), Snyk (lockfile-parser → graph), and Dependabot (file-fetcher → file-parser) all separate file reading from interpretation.

### Layer 1: File Parsers

Each parser is responsible for exactly one file type. It reads the file and emits a `Vec<Fact>` — typed, structured observations with no interpretation. A single file can emit multiple fact types (e.g., a Dockerfile yields both Node.js and Ubuntu runtime versions).

```
enum Fact {
    /// Pinned dependency from a lockfile (high confidence)
    Dependency { name: String, version: String, ecosystem: Ecosystem },

    /// Dependency range from a manifest (low confidence, used as fallback)
    DependencyHint { name: String, range: String, ecosystem: Ecosystem },

    /// Concrete runtime version (e.g., "20.11.0" from .nvmrc)
    RuntimeVersion { runtime: Runtime, version: String, confidence: Confidence, source: FilePath },

    /// Unresolved runtime alias (e.g., "lts/iron" from .nvmrc — needs secondary lookup)
    RuntimeAlias { runtime: Runtime, alias: String, source: FilePath },
}

enum Runtime { NodeJs, Python, Java, Ubuntu, Alpine, Debian }

enum Confidence { High, Medium, Low }
```

**Why confidence matters:** `.nvmrc` with a pinned version is high confidence — it's an explicit declaration. `engines.node: ">=18"` is low confidence — it's a compatibility range, not a runtime declaration. The resolver uses confidence to pick the best source when multiple files declare the same runtime.

**Source priority (used to break ties at the same confidence level):**

| Runtime | Priority (highest first) |
|---------|-------------------------|
| Node.js | `.nvmrc` / `.node-version` → `Dockerfile` → `package.json` `engines.node` |
| Python  | `.python-version` → `runtime.txt` → `pyproject.toml` |
| Java    | `pom.xml` → `build.gradle` / `build.gradle.kts` |
| OS      | `Dockerfile` (only source) |

### Layer 2: File Parser Inventory

#### `package-lock.json` → `Dependency` facts

Parses npm lockfile for top-level dependencies with pinned versions.

- **v2/v3 format (npm 7+):** Read the `packages` map. The root entry is `""`, dependencies are keyed as `node_modules/{name}`. Extract `version` field from each top-level entry. This is the primary format.
- **v1 format (npm 6):** Fall back to the `dependencies` map at the top level. Extract `version` field from each entry.
- **Detection:** Check `"lockfileVersion"` field (1, 2, or 3). v2 lockfiles contain both `packages` and `dependencies` for backwards compatibility — prefer `packages`.
- Emits: `Fact::Dependency` for each top-level package with its pinned version.

#### `package.json` → `DependencyHint` + `RuntimeVersion` facts

- **Dependencies:** Read `dependencies` and `devDependencies` maps. Emit `Fact::DependencyHint` with the version range string. These are only used if no lockfile is present.
- **Runtime:** Read `engines.node` if present. This is a semver range (e.g., `">=18"`, `"^20.0.0"`). Emit `Fact::RuntimeVersion` with `confidence: Low` and the minimum satisfying major version extracted from the range.
- **Why low confidence:** The `engines` field is advisory by default (npm only enforces it with `engine-strict=true` in `.npmrc`). It documents compatibility intent, not actual runtime version.

#### `.nvmrc` / `.node-version` → `RuntimeVersion` or `RuntimeAlias` facts

Supported formats:
- Exact version: `20.11.0`, `20.11`, `20` → `Fact::RuntimeVersion` with `confidence: High`
- With `v` prefix: `v20.11.0` → strip prefix, same as above
- LTS aliases: `lts/*`, `lts/iron` → `Fact::RuntimeAlias` (cannot resolve without secondary lookup)
- Special aliases: `node` (latest), `stable` → `Fact::RuntimeAlias`
- Comments: lines starting with `#` are ignored
- Whitespace: leading/trailing whitespace is trimmed

#### `Dockerfile` → `RuntimeVersion` facts

Parses Dockerfile with `ARG` expansion and multi-stage awareness.

**Parsing strategy (modeled after Renovate's Dockerfile manager):**

1. **First pass — collect global `ARG` values:**
   Scan lines before the first `FROM` for `ARG NAME=value` declarations. Store in a map.

2. **Second pass — process `FROM` lines with substitution:**
   For each `FROM image:tag` line, substitute any `${VAR}` or `$VAR` references using the collected ARG map. Then match the resolved image name against known runtime images.

3. **Multi-stage handling:**
   Track all stages by collecting `AS <name>` aliases from each `FROM ... AS <name>` line. Use the **last `FROM`** as the runtime stage (this is the image that actually runs). Earlier stages are build stages. When processing a `FROM` line, check if the image name matches a previously declared stage alias — if so, skip it (it's an internal reference, not an external image).

**Supported image patterns:**

| Image | Runtime | Version extraction |
|-------|---------|-------------------|
| `node:20`, `node:20-alpine`, `node:20.11.0-slim` | Node.js | First numeric segment from tag |
| `ubuntu:22.04` | Ubuntu | Full tag (e.g., `22.04`) |
| `alpine:3.19` | Alpine | Full tag |
| `debian:bookworm`, `debian:12` | Debian | Tag or codename (see codename map below) |
| `python:3.12`, `python:3.12-slim` | Python | Major.minor from tag |

**Unresolvable cases (emit nothing, log with `--verbose`):**
- `FROM node` (no tag — implies `latest`, version unknown)
- `FROM node:${VERSION}` where `VERSION` has no default ARG value
- `FROM myregistry.com/custom-image:v3` (not a known runtime image)
- `FROM node:lts` or `FROM node:lts-alpine` (alias, not a version)

**Debian codename map (hardcoded):**

```
bookworm → 12, bullseye → 11, buster → 10, trixie → 13
```

Update this map when new Debian releases ship. It's a small, stable list.

#### `.python-version` / `runtime.txt` → `RuntimeVersion` facts

- `.python-version`: single version string, e.g., `3.12.1` → `Fact::RuntimeVersion { runtime: Python, confidence: High }`
- `runtime.txt`: format `python-3.12.1` → strip `python-` prefix, same as above

#### `pyproject.toml` → `RuntimeVersion` facts

- Read `requires-python` from `[project]` table using a proper TOML parser (not regex).
- Value is a PEP 440 version specifier (e.g., `">=3.10"`). Extract the minimum major.minor version.
- Emit `Fact::RuntimeVersion { runtime: Python, confidence: Medium }` — it's a constraint, not a pinned version, but more authoritative than `engines.node` since Python tooling actually enforces it.

#### `pom.xml` → `RuntimeVersion` facts

- Read `<maven.compiler.source>`, `<maven.compiler.target>`, or `<maven.compiler.release>` from `<properties>`.
- Also check `<source>`, `<target>`, `<release>` inside `<maven-compiler-plugin>` configuration.
- If the value is a property reference (`${java.version}`), resolve it from `<properties>`. One level of indirection only — nested property references are not resolved.
- Emit `Fact::RuntimeVersion { runtime: Java, confidence: Medium }`.

#### `build.gradle` → `RuntimeVersion` facts

- Applies to both `build.gradle` (Groovy DSL) and `build.gradle.kts` (Kotlin DSL).
- Match `sourceCompatibility = '17'` or `sourceCompatibility = JavaVersion.VERSION_17` via regex.
- Also match `java { toolchain { languageVersion = JavaLanguageVersion.of(21) } }`.
- Emit `Fact::RuntimeVersion { runtime: Java, confidence: Medium }`.
- Variable references (e.g., `sourceCompatibility = myVar`) are not resolved — emit nothing, log with `--verbose`.

### Layer 3: Resolver

The resolver collects all facts from all parsers and produces the inputs for the signal generation phase.

#### Dependency Resolution

1. If `Dependency` facts exist (from lockfile): use them. These have pinned versions.
2. If only `DependencyHint` facts exist (from `package.json`): use them with a warning in `--verbose` output that versions are ranges, not pinned. For registry lookups, use the range string as-is — the registry response gives us `dist-tags.latest` regardless. For version comparison, extract the minimum satisfying version from the range (e.g., `^4.18.0` → `4.18.0`) using semver parsing. If the range is unparseable, skip `BehindMajor`/`BehindMinor` signals for that package and emit an Info-level diagnostic.
3. If neither exists: no npm dependencies to check.

#### Runtime Resolution

For each runtime (Node.js, Python, Java, Ubuntu, Alpine, Debian):

1. Collect all `RuntimeVersion` and `RuntimeAlias` facts for that runtime.
2. If multiple sources exist, pick the highest-confidence one. On confidence tie, prefer the more specific source (`.nvmrc` > `Dockerfile` > `package.json`).
3. If sources conflict (e.g., `.nvmrc` says 18, Dockerfile says 20), use the highest-confidence one and emit a `Conflict` diagnostic in `--verbose` output showing all sources.
4. If only `RuntimeAlias` facts exist (e.g., `lts/iron`), resolve via the Node.js release schedule API (`https://nodejs.org/dist/index.json`) to map LTS codenames to version numbers. If resolution fails (network error, unknown alias), emit an Info-level signal and skip EOL checking for that runtime.

#### Version Normalization

The endoflife.date API has inconsistent version formats across products (issue #4879). Normalize before matching:

- Strip `v` prefix
- For cycle matching: use major version for Node.js and Java (e.g., `20.11.0` → cycle `20`), major.minor for Python and Alpine (e.g., `3.12.1` → cycle `3.12`), major.minor for Ubuntu (e.g., `22.04`), major for Debian (e.g., `12`)

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
    eol_date: Option<NaiveDate>,  // None when endoflife.date returns eol: false
    days_left: Option<i64>,       // None when no EOL date; negative = past EOL
    cycle: String,                // e.g. "20" for Node 20
    ref_url: String,              // link to endoflife.date
}
```

### Fact

Intermediate representation emitted by file parsers. See Architecture section above.

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

## Core Flow

1. **Load config** — CLI flags override `.shelflife.toml`, which overrides defaults
2. **Discover files** — scan target directory root (no recursion) for known file names (`package-lock.json`, `package.json`, `Dockerfile`, `.nvmrc`, `.node-version`, `.python-version`, `runtime.txt`, `pyproject.toml`, `pom.xml`, `build.gradle`, `build.gradle.kts`)
3. **Parse files → facts** — run each discovered file through its parser, collect all `Fact` values
4. **Resolve facts** — deduplicate, pick best source per runtime, resolve aliases
5. **Lookup npm registry** — `GET https://registry.npmjs.org/{pkg}` for each resolved dependency (concurrent, bounded to ~8 in-flight)
6. **Check runtime EOL** — `GET https://endoflife.date/api/{product}.json` for each resolved runtime, match normalized version to cycle
7. **Generate signals** — classify each finding: deprecated, stale, behind-major, behind-minor, EOL, approaching-EOL
8. **Output** — render colored terminal table (default) or JSON (`--json`)
9. **Exit** — code 0 if no findings at configured `--fail-on` level, code 1 otherwise

## Registry & API Integration

### npm Registry

- Endpoint: `GET https://registry.npmjs.org/{package}`
- Extract: `dist-tags.latest`, `time.{version}` (publish dates), `versions.{v}.deprecated`
- Concurrency: max 8 in-flight requests
- Timeout: 10s per request
- No retries in v1; failed lookups produce an Info-level `RegistryError` signal

### endoflife.date API

- Endpoint: `GET https://endoflife.date/api/{product}.json`
- **Product slug mapping (explicit, not derived from runtime name):**

  | Runtime | API slug |
  |---------|----------|
  | Node.js | `nodejs` |
  | Python  | `python` |
  | Java    | `java`   |
  | Ubuntu  | `ubuntu` |
  | Alpine  | `alpine` |
  | Debian  | `debian` |

- Match normalized version to a cycle, read `eol` date field
- `eol` can be `false` (boolean) when no EOL date is set — treat as "not EOL, no signal"
- `eol` can be a date string (`"2025-04-30"`) — parse as `NaiveDate`
- No retries in v1; failed lookups produce an Info-level signal and don't block the scan
- Timeout: 10s per request

### Node.js Release Schedule (for alias resolution)

- Endpoint: `GET https://nodejs.org/dist/index.json`
- Used only when `.nvmrc` contains an LTS alias (e.g., `lts/iron`)
- Map codename to latest version in that LTS line
- Cache response for the duration of the scan (single request)
- If unavailable, emit Info-level signal, skip EOL check for that runtime

## Signal Generation

For each npm package:
- **Deprecated**: registry metadata has deprecation message → `Critical`
- **Stale**: the `dist-tags.latest` version's publish date is older than `stale_months` (measures project activity, not how old your installed version is) → `Warning`
- **BehindMajor**: installed major < latest major → `Warning`
- **BehindMinor**: installed minor < latest minor (same major) → `Warning`
- **NotFound**: package not in registry → `Info`
- **RegistryError**: HTTP error fetching metadata → `Info`

For each runtime:
- **Eol**: `days_left < 0` → `Critical`
- **ApproachingEol**: `0 < days_left < threshold_days` → `Warning`

## Output Formats

### Terminal (default)

Colored table with status indicators:

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

With `--verbose`, also shows:
- Which file each runtime version was detected from and its confidence level
- Conflicts between sources (e.g., ".nvmrc says 18, Dockerfile says 20 — using .nvmrc (higher confidence)")
- Warnings about range-only dependency versions (no lockfile)
- Unresolvable files (ARG references, unknown aliases)

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

With `--fail-on any` (default): any finding with Warning or Critical severity triggers exit 1.
With `--fail-on critical`: only critical findings (deprecated, past EOL) trigger exit 1.
With `--fail-on none`: always exits 0.

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
      package_json.rs    # package.json parser (deps + engines)
      nvmrc.rs           # .nvmrc / .node-version parser
      dockerfile.rs      # Dockerfile parser (ARG expansion, multi-stage)
      python.rs          # .python-version, runtime.txt, pyproject.toml
      java.rs            # pom.xml, build.gradle
    resolver.rs          # Fact deduplication, conflict resolution, alias resolution
    registries/
      mod.rs             # Shared HTTP client, concurrency control
      npm.rs             # npm registry client
      eol.rs             # endoflife.date API client
      node_releases.rs   # nodejs.org release schedule (for alias resolution)
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
| `toml` | Config file parsing (for `.shelflife.toml` and `pyproject.toml`) |
| `tabled` | Terminal table rendering |
| `owo-colors` | Terminal color output |
| `chrono` | Date handling and arithmetic |

## Out of Scope

These are explicitly excluded from the initial version and may be added later:

- **PyPI / Maven / Gradle registry lookups** — runtimes are detected from config files but package registries for these ecosystems are not queried
- **Monorepo / workspace scanning** — only scans the target directory root, does not recurse into workspace packages
- **`npm ls` invocation** — lockfile parsing only, no subprocess calls (consistent with npm audit, Snyk, Renovate)
- **`pnpm-lock.yaml` / `yarn.lock`** — only `package-lock.json` is supported for pinned dependency resolution; pnpm and yarn users fall back to `package.json` ranges with degraded accuracy
- **Report file generation** — no Markdown, CSV, or HTML output files
- **Audit trails** — no scan history or SOC2 logging
- **GitHub App integration** — this is a local CLI only
- **HTTP caching / rate limiting** — rely on connection pooling and concurrency limits
- **Lock file generation** — read-only tool, does not modify the project
- **Local EOL database** — queries endoflife.date API directly (xeol-style local DB caching is a future optimization)
