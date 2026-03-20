use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// -- Facts (parser output) --

#[derive(Debug, Clone)]
pub enum Fact {
    Dependency {
        name: String,
        version: String,
    },
    RuntimeVersion {
        runtime: Runtime,
        version: String,
        source: String,
    },
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
            SignalKind::Stale
            | SignalKind::BehindMajor
            | SignalKind::BehindMinor
            | SignalKind::ApproachingEol => Severity::Warning,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FailOn {
    Any,
    Critical,
    None,
}

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
            counts: Counts {
                total: 0,
                critical: 0,
                warning: 0,
                ok: 0,
            },
            scanned_at: Utc::now(),
            path: PathBuf::from("."),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"findings\":[]"));
    }
}
