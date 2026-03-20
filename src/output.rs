use crate::model::{Counts, Ecosystem, Finding, ScanResult, Severity, Signal};
use owo_colors::OwoColorize;

/// Returns a colored status label based on the maximum signal severity in the finding.
pub fn finding_status(f: &Finding) -> String {
    let max_severity = f
        .signals
        .iter()
        .map(|s| s.severity)
        .max()
        .unwrap_or(Severity::Info);

    match max_severity {
        Severity::Critical => "[CRITICAL]".red().bold().to_string(),
        Severity::Warning => "[WARNING]".yellow().bold().to_string(),
        Severity::Info => "[OK]".green().to_string(),
    }
}

/// Pretty-printed JSON output.
pub fn format_json(result: &ScanResult) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(result)
}

/// Colored terminal output with runtime EOL section, npm deps section, and summary line.
pub fn format_terminal(result: &ScanResult) -> String {
    let mut out = String::new();

    // Runtime EOL section
    let runtime_findings: Vec<&Finding> = result
        .findings
        .iter()
        .filter(|f| f.ecosystem == Ecosystem::Runtime)
        .collect();

    if !runtime_findings.is_empty() {
        out.push_str(&format!("{}\n", "Runtime EOL".bold().underline()));
        for f in &runtime_findings {
            let status = finding_status(f);
            let signals_str = format_signals(&f.signals);
            out.push_str(&format!(
                "  {} {} {}  {}\n",
                status,
                f.name.bold(),
                f.installed_version,
                signals_str
            ));
        }
        out.push('\n');
    }

    // npm deps section
    let npm_findings: Vec<&Finding> = result
        .findings
        .iter()
        .filter(|f| f.ecosystem == Ecosystem::Npm)
        .collect();

    if !npm_findings.is_empty() {
        out.push_str(&format!("{}\n", "npm Dependencies".bold().underline()));
        for f in &npm_findings {
            let status = finding_status(f);
            let latest = f
                .latest_version
                .as_deref()
                .map(|v| format!(" → {v}"))
                .unwrap_or_default();
            let signals_str = format_signals(&f.signals);
            out.push_str(&format!(
                "  {} {}@{}{} {}\n",
                status,
                f.name.bold(),
                f.installed_version,
                latest,
                signals_str
            ));
        }
        out.push('\n');
    }

    // Summary line
    out.push_str(&format_summary(&result.counts));

    out
}

fn format_signals(signals: &[Signal]) -> String {
    if signals.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = signals.iter().map(|s| s.message.clone()).collect();
    format!("({})", parts.join(", "))
}

fn format_summary(counts: &Counts) -> String {
    format!(
        "Summary: {} total, {} critical, {} warning, {} ok\n",
        counts.total,
        counts.critical.to_string().red().bold(),
        counts.warning.to_string().yellow().bold(),
        counts.ok.to_string().green(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Counts, Ecosystem, Finding, ScanResult};
    use chrono::Utc;
    use std::path::PathBuf;

    fn make_result() -> ScanResult {
        ScanResult {
            findings: vec![],
            counts: Counts {
                total: 0,
                critical: 0,
                warning: 0,
                ok: 0,
            },
            scanned_at: Utc::now(),
            path: PathBuf::from("."),
        }
    }

    #[test]
    fn json_output_is_valid_json() {
        let result = make_result();
        let json_str = format_json(&result).expect("should serialize");
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("should be valid JSON");
        assert!(parsed.get("findings").is_some());
        assert!(parsed.get("counts").is_some());
    }

    #[test]
    fn terminal_output_contains_summary() {
        let result = ScanResult {
            findings: vec![Finding {
                ecosystem: Ecosystem::Npm,
                name: "express".into(),
                installed_version: "4.18.2".into(),
                latest_version: Some("5.0.0".into()),
                signals: vec![],
                eol_info: None,
            }],
            counts: Counts {
                total: 1,
                critical: 0,
                warning: 0,
                ok: 1,
            },
            scanned_at: Utc::now(),
            path: PathBuf::from("."),
        };
        let output = format_terminal(&result);
        assert!(output.contains("Summary:"));
        assert!(output.contains("1 total"));
    }
}
