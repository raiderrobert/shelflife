#![allow(dead_code)]

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

const NPM_REGISTRY: &str = "https://registry.npmjs.org";
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct NpmPackageInfo {
    pub latest_version: String,
    pub deprecated: bool,
    pub latest_publish_date: Option<DateTime<Utc>>,
}

impl NpmPackageInfo {
    pub fn from_registry_response(json: &Value) -> Option<Self> {
        let latest = json.get("dist-tags")?.get("latest")?.as_str()?;

        let deprecated = json
            .get("versions")
            .and_then(|v| v.get(latest))
            .and_then(|v| v.get("deprecated"))
            .is_some();

        let publish_date = json
            .get("time")
            .and_then(|t| t.get(latest))
            .and_then(|d| d.as_str())
            .and_then(|d| d.parse::<DateTime<Utc>>().ok());

        Some(Self {
            latest_version: latest.to_string(),
            deprecated,
            latest_publish_date: publish_date,
        })
    }
}

#[derive(Debug)]
pub enum NpmError {
    NotFound,
    Http(String),
    ParseError,
}

pub async fn fetch_package(client: &Client, name: &str) -> Result<NpmPackageInfo, NpmError> {
    let url = format!("{NPM_REGISTRY}/{name}");
    let resp = client
        .get(&url)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| NpmError::Http(e.to_string()))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(NpmError::NotFound);
    }

    let json: Value = resp
        .json()
        .await
        .map_err(|e| NpmError::Http(e.to_string()))?;

    NpmPackageInfo::from_registry_response(&json).ok_or(NpmError::ParseError)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_npm_response_normal_package() {
        let json = serde_json::json!({
            "dist-tags": { "latest": "4.18.2" },
            "time": {
                "4.18.2": "2024-10-01T00:00:00.000Z"
            },
            "versions": {
                "4.18.2": {}
            }
        });
        let info = NpmPackageInfo::from_registry_response(&json).unwrap();
        assert_eq!(info.latest_version, "4.18.2");
        assert!(!info.deprecated);
        assert!(info.latest_publish_date.is_some());
    }

    #[test]
    fn parse_npm_response_deprecated() {
        let json = serde_json::json!({
            "dist-tags": { "latest": "2.88.2" },
            "time": { "2.88.2": "2020-03-15T00:00:00.000Z" },
            "versions": {
                "2.88.2": { "deprecated": "request has been deprecated" }
            }
        });
        let info = NpmPackageInfo::from_registry_response(&json).unwrap();
        assert!(info.deprecated);
    }

    #[test]
    fn parse_npm_response_missing_time() {
        let json = serde_json::json!({
            "dist-tags": { "latest": "1.0.0" },
            "versions": { "1.0.0": {} }
        });
        let info = NpmPackageInfo::from_registry_response(&json).unwrap();
        assert!(info.latest_publish_date.is_none());
    }
}
