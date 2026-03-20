#![allow(dead_code)]

use crate::model::{EolInfo, Severity, Signal, SignalKind};
use crate::registries::npm::NpmPackageInfo;

pub fn npm_signals(
    installed_version: &str,
    info: &NpmPackageInfo,
    stale_months: u32,
) -> Vec<Signal> {
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
                message: format!(
                    "EOL {} (expired {} days ago)",
                    eol_info.eol_date.unwrap(),
                    -days_left
                ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries::npm::NpmPackageInfo;
    use chrono::{Duration, NaiveDate, Utc};

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
