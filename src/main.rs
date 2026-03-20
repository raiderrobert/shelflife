mod config;
mod model;
mod output;
mod parsers;
mod registries;
mod resolver;
mod signal;

use clap::Parser;
use config::{CliOverrides, Config};
use model::{
    Counts, Ecosystem, EolInfo, FailOn, Finding, ScanResult, Severity, Signal, SignalKind,
};
use registries::eol::fetch_eol;
use registries::npm::{fetch_package, NpmError};
use reqwest::Client;
use resolver::resolve;
use signal::{eol_signals, npm_signals};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Debug, Parser)]
#[command(name = "shelflife", version, about = "Dependency freshness checker")]
struct Cli {
    /// Path to the project to scan
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Path to config file (defaults to <path>/.shelflife.toml)
    #[arg(short = 'c', long)]
    config: Option<PathBuf>,

    /// Number of days without a new release before marking stale
    #[arg(long)]
    threshold_days: Option<u32>,

    /// Number of months before marking a runtime as approaching EOL
    #[arg(long)]
    stale_months: Option<u32>,

    /// Comma-separated list of packages to ignore
    #[arg(long, value_delimiter = ',')]
    ignore: Option<Vec<String>>,

    /// Output results as JSON
    #[arg(long, default_value_t = false)]
    json: bool,

    /// Which finding severity causes a non-zero exit code
    #[arg(long, value_parser = parse_fail_on)]
    fail_on: Option<FailOn>,

    /// Verbose output
    #[arg(short = 'v', long, default_value_t = false)]
    verbose: bool,
}

fn parse_fail_on(s: &str) -> Result<FailOn, String> {
    match s {
        "any" => Ok(FailOn::Any),
        "critical" => Ok(FailOn::Critical),
        "none" => Ok(FailOn::None),
        other => Err(format!(
            "invalid value '{other}': expected one of any, critical, none"
        )),
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(|| cli.path.join(".shelflife.toml"));

    let base = if config_path.exists() {
        match Config::from_toml(&config_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error loading config: {e}");
                std::process::exit(1);
            }
        }
    } else {
        Config {
            path: cli.path.clone(),
            ..Config::default()
        }
    };

    let overrides = CliOverrides {
        threshold_days: cli.threshold_days,
        stale_months: cli.stale_months,
        ignore: cli.ignore,
        fail_on: cli.fail_on,
        json: cli.json,
        verbose: cli.verbose,
    };

    let config = base.merge(overrides);

    // Parse directory for facts
    let facts = parsers::parse_directory(&config.path);

    if config.verbose {
        eprintln!(
            "[verbose] parsed {} facts from {:?}",
            facts.len(),
            config.path
        );
    }

    // Resolve facts into dependencies and runtimes
    let resolved = resolve(facts);

    if config.verbose {
        for (runtime, rv) in &resolved.runtimes {
            eprintln!(
                "[verbose] runtime {:?} resolved from {}",
                runtime, rv.source
            );
        }
        if resolved.runtimes.is_empty() {
            eprintln!("[verbose] no lockfile found; skipping runtime detection");
        }
    }

    let client = Client::new();
    let semaphore = Arc::new(Semaphore::new(8));
    let mut findings: Vec<Finding> = Vec::new();

    // Fetch npm registry info concurrently
    let mut npm_tasks = Vec::new();
    for dep in &resolved.dependencies {
        if config.ignore.contains(&dep.name) {
            continue;
        }
        let name = dep.name.clone();
        let version = dep.version.clone();
        let client = client.clone();
        let sem = semaphore.clone();
        let stale_months = config.stale_months;

        npm_tasks.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            let result = fetch_package(&client, &name).await;
            (name, version, stale_months, result)
        }));
    }

    for task in npm_tasks {
        let (name, installed_version, stale_months, result) = task.await.expect("task panicked");
        let (signals, latest_version): (Vec<Signal>, Option<String>) = match result {
            Ok(info) => {
                let latest = info.latest_version.clone();
                let sigs = npm_signals(&installed_version, &info, stale_months);
                (sigs, Some(latest))
            }
            Err(NpmError::NotFound) => (
                vec![Signal {
                    kind: SignalKind::NotFound,
                    severity: Severity::Info,
                    message: "package not found in registry".into(),
                }],
                None,
            ),
            Err(NpmError::Http(msg)) => (
                vec![Signal {
                    kind: SignalKind::RegistryError,
                    severity: Severity::Info,
                    message: format!("registry error: {msg}"),
                }],
                None,
            ),
            Err(NpmError::ParseError) => (
                vec![Signal {
                    kind: SignalKind::RegistryError,
                    severity: Severity::Info,
                    message: "failed to parse registry response".into(),
                }],
                None,
            ),
        };

        findings.push(Finding {
            ecosystem: Ecosystem::Npm,
            name,
            installed_version,
            latest_version,
            signals,
            eol_info: None,
        });
    }

    // Fetch EOL info for runtimes
    for (runtime, resolved_version) in &resolved.runtimes {
        let runtime_name = format!("{:?}", runtime).to_lowercase();
        let eol_result = fetch_eol(&client, *runtime, &resolved_version.version).await;

        let (signals, eol_info): (Vec<Signal>, Option<EolInfo>) = match eol_result {
            Ok(info) => {
                let sigs = eol_signals(&info, config.threshold_days);
                (sigs, Some(info))
            }
            Err(e) => (
                vec![Signal {
                    kind: SignalKind::RegistryError,
                    severity: Severity::Info,
                    message: format!("EOL lookup failed: {e}"),
                }],
                None,
            ),
        };

        findings.push(Finding {
            ecosystem: Ecosystem::Runtime,
            name: runtime_name,
            installed_version: resolved_version.version.clone(),
            latest_version: None,
            signals,
            eol_info,
        });
    }

    // Count severities
    let total = findings.len();
    let critical = findings
        .iter()
        .filter(|f| f.signals.iter().any(|s| s.severity == Severity::Critical))
        .count();
    let warning = findings
        .iter()
        .filter(|f| {
            !f.signals.iter().any(|s| s.severity == Severity::Critical)
                && f.signals.iter().any(|s| s.severity == Severity::Warning)
        })
        .count();
    let ok = total - critical - warning;

    let counts = Counts {
        total,
        critical,
        warning,
        ok,
    };

    let result = ScanResult {
        findings,
        counts: counts.clone(),
        scanned_at: chrono::Utc::now(),
        path: config.path.clone(),
    };

    // Output
    if config.json {
        match output::format_json(&result) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("error serializing JSON: {e}");
                std::process::exit(1);
            }
        }
    } else {
        print!("{}", output::format_terminal(&result));
    }

    // Determine exit code
    let exit_code = match config.fail_on {
        FailOn::Any => {
            if counts.critical > 0 || counts.warning > 0 {
                1
            } else {
                0
            }
        }
        FailOn::Critical => {
            if counts.critical > 0 {
                1
            } else {
                0
            }
        }
        FailOn::None => 0,
    };

    std::process::exit(exit_code);
}
