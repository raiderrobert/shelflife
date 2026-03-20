mod config;
mod model;
mod parsers;

use clap::Parser;
use config::{CliOverrides, Config};
use model::FailOn;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "shelflife", about = "Dependency freshness checker")]
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

fn main() {
    let cli = Cli::parse();

    let config_path = cli
        .config
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

    let _config = base.merge(overrides);

    // TODO: invoke scanner with _config
    println!("shelflife v0.1.0");
}
