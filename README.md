# shelflife

Check your dependencies and runtimes for end-of-life risk.

- **npm dependency scanning.** Checks every package in your lockfile against the npm registry for deprecation, staleness, and version drift.
- **Runtime EOL detection.** Finds your Node.js, Python, and Java versions from config files and checks them against [endoflife.date](https://endoflife.date).
- **Zero config.** Point it at a directory and go. Optional `.shelflife.toml` for team-wide settings.
- **CI-ready.** `--fail-on critical` exits non-zero only on deprecated packages or expired runtimes. `--json` for machine-readable output.
- **Fast.** Single binary, concurrent registry lookups, no runtime dependencies.

## Install

Build from source:

```bash
cargo install --path .
```

Or grab a binary from [GitHub Releases](https://github.com/raiderrobert/shelflife/releases).

## Quick Start

```bash
shelflife
```

That's it. Shelflife scans the current directory, checks your lockfile and runtime config files, and prints a summary:

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

## Usage

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

## Configuration

Optional `.shelflife.toml` in your project root:

```toml
threshold_days = 180
stale_months = 18
ignore = ["some-internal-pkg"]
fail_on = "any"  # any | critical | none
```

CLI flags override the config file.

## What It Checks

**npm packages** (from `package-lock.json`):
- Deprecated packages
- Major/minor versions behind latest
- Stale projects (latest version not published in 18+ months)

**Runtimes** (detected from config files):
| Runtime | Detected from |
|---------|--------------|
| Node.js | `.nvmrc`, `.node-version`, `package.json` `engines.node` |
| Python  | `.python-version`, `runtime.txt`, `pyproject.toml` |
| Java    | `pom.xml`, `build.gradle` |

## License

[PolyForm Shield 1.0.0](LICENSE)
