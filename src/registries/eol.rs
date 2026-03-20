#![allow(dead_code)]

use crate::model::{EolInfo, Runtime};
use chrono::NaiveDate;
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

const EOL_API: &str = "https://endoflife.date/api";
const TIMEOUT: Duration = Duration::from_secs(10);

pub fn normalize_version(runtime: Runtime, version: &str) -> String {
    let v = version.strip_prefix('v').unwrap_or(version);
    let parts: Vec<&str> = v.split('.').collect();
    match runtime {
        Runtime::NodeJs | Runtime::Java => parts[0].to_string(),
        Runtime::Python => {
            if parts.len() >= 2 {
                format!("{}.{}", parts[0], parts[1])
            } else {
                parts[0].to_string()
            }
        }
    }
}

#[derive(Debug)]
pub struct CycleInfo {
    pub cycle: String,
    pub eol_date: Option<NaiveDate>,
}

pub fn find_cycle(cycles: &Value, target_cycle: &str) -> Option<CycleInfo> {
    let arr = cycles.as_array()?;
    for entry in arr {
        let cycle = entry.get("cycle")?.as_str()?;
        if cycle == target_cycle {
            let eol_date = match entry.get("eol")? {
                Value::String(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d").ok(),
                Value::Bool(false) => None,
                _ => None,
            };
            return Some(CycleInfo {
                cycle: cycle.to_string(),
                eol_date,
            });
        }
    }
    None
}

pub async fn fetch_eol(
    client: &Client,
    runtime: Runtime,
    version: &str,
) -> Result<EolInfo, String> {
    let slug = runtime.eol_slug();
    let cycle = normalize_version(runtime, version);
    let url = format!("{EOL_API}/{slug}.json");

    let resp = client
        .get(&url)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let json: Value = resp.json().await.map_err(|e| e.to_string())?;

    let info =
        find_cycle(&json, &cycle).ok_or_else(|| format!("cycle {cycle} not found for {slug}"))?;

    let days_left = info.eol_date.map(|eol| {
        let today = chrono::Utc::now().date_naive();
        (eol - today).num_days()
    });

    Ok(EolInfo {
        eol_date: info.eol_date,
        days_left,
        cycle: info.cycle,
        ref_url: format!("https://endoflife.date/{slug}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn parse_cycle_with_date_eol() {
        let json = serde_json::json!([
            { "cycle": "20", "eol": "2026-04-30", "latest": "20.18.0" },
            { "cycle": "18", "eol": "2025-04-30", "latest": "18.20.4" }
        ]);
        let info = find_cycle(&json, "18").unwrap();
        assert_eq!(info.cycle, "18");
        assert_eq!(
            info.eol_date,
            Some(NaiveDate::from_ymd_opt(2025, 4, 30).unwrap())
        );
    }

    #[test]
    fn parse_cycle_with_false_eol() {
        let json = serde_json::json!([
            { "cycle": "22", "eol": false, "latest": "22.12.0" }
        ]);
        let info = find_cycle(&json, "22").unwrap();
        assert_eq!(info.eol_date, None);
    }

    #[test]
    fn cycle_not_found() {
        let json = serde_json::json!([
            { "cycle": "20", "eol": "2026-04-30" }
        ]);
        assert!(find_cycle(&json, "99").is_none());
    }

    #[test]
    fn normalize_version_nodejs() {
        assert_eq!(normalize_version(Runtime::NodeJs, "20.11.0"), "20");
        assert_eq!(normalize_version(Runtime::NodeJs, "v18.19.0"), "18");
    }

    #[test]
    fn normalize_version_python() {
        assert_eq!(normalize_version(Runtime::Python, "3.12.1"), "3.12");
        assert_eq!(normalize_version(Runtime::Python, "3.10"), "3.10");
    }

    #[test]
    fn normalize_version_java() {
        assert_eq!(normalize_version(Runtime::Java, "17"), "17");
        assert_eq!(normalize_version(Runtime::Java, "21.0.1"), "21");
    }
}
