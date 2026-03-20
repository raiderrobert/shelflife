use crate::model::FailOn;
use serde::Deserialize;
use std::path::{Path, PathBuf};

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
        Config {
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

#[derive(Debug, Deserialize)]
struct TomlConfig {
    threshold_days: Option<u32>,
    stale_months: Option<u32>,
    ignore: Option<Vec<String>>,
    fail_on: Option<FailOn>,
}

#[derive(Debug, Default)]
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
        let contents = std::fs::read_to_string(path)?;
        let toml_cfg: TomlConfig = toml::from_str(&contents)?;
        let defaults = Config::default();
        Ok(Config {
            threshold_days: toml_cfg.threshold_days.unwrap_or(defaults.threshold_days),
            stale_months: toml_cfg.stale_months.unwrap_or(defaults.stale_months),
            ignore: toml_cfg.ignore.unwrap_or(defaults.ignore),
            fail_on: toml_cfg.fail_on.unwrap_or(defaults.fail_on),
            json: defaults.json,
            verbose: defaults.verbose,
            path: path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from(".")),
        })
    }

    pub fn merge(self, cli: CliOverrides) -> Self {
        Config {
            threshold_days: cli.threshold_days.unwrap_or(self.threshold_days),
            stale_months: cli.stale_months.unwrap_or(self.stale_months),
            ignore: cli.ignore.unwrap_or(self.ignore),
            fail_on: cli.fail_on.unwrap_or(self.fail_on),
            json: cli.json,
            verbose: cli.verbose,
            path: self.path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn defaults() {
        let cfg = Config::default();
        assert_eq!(cfg.threshold_days, 180);
        assert_eq!(cfg.stale_months, 18);
        assert!(cfg.ignore.is_empty());
        assert_eq!(cfg.fail_on, FailOn::Any);
        assert!(!cfg.json);
        assert!(!cfg.verbose);
    }

    #[test]
    fn from_toml_file() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/config/shelflife.toml");
        let cfg = Config::from_toml(&fixture).expect("should load fixture");
        assert_eq!(cfg.threshold_days, 90);
        assert_eq!(cfg.stale_months, 12);
        assert_eq!(cfg.ignore, vec!["internal-pkg"]);
        assert_eq!(cfg.fail_on, FailOn::Critical);
    }

    #[test]
    fn merge_overrides_toml_with_cli() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/config/shelflife.toml");
        let cfg = Config::from_toml(&fixture).expect("should load fixture");
        let cli = CliOverrides {
            threshold_days: Some(30),
            ..Default::default()
        };
        let merged = cfg.merge(cli);
        assert_eq!(merged.threshold_days, 30);
        assert_eq!(merged.stale_months, 12);
    }

    #[test]
    fn missing_toml_returns_error() {
        let result = Config::from_toml(Path::new("/nonexistent/path/shelflife.toml"));
        assert!(result.is_err());
    }
}
