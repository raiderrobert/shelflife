use crate::model::Fact;
use serde_json::Value;
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let parsed: Value = serde_json::from_str(&content)?;

    let version = parsed
        .get("lockfileVersion")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);

    if version >= 2 {
        if let Some(packages) = parsed.get("packages").and_then(|p| p.as_object()) {
            return Ok(packages_field(packages));
        }
    }

    // v1 fallback or v2 without packages field
    if let Some(deps) = parsed.get("dependencies").and_then(|d| d.as_object()) {
        return Ok(dependencies_field(deps));
    }

    Ok(vec![])
}

fn packages_field(packages: &serde_json::Map<String, Value>) -> Vec<Fact> {
    packages
        .iter()
        .filter_map(|(key, value)| {
            // Top-level deps are "node_modules/{name}" (no nested /)
            let name = key.strip_prefix("node_modules/")?;
            if name.contains("/node_modules/") || name.is_empty() {
                return None;
            }
            let version = value.get("version")?.as_str()?.to_string();
            Some(Fact::Dependency {
                name: name.to_string(),
                version,
            })
        })
        .collect()
}

fn dependencies_field(deps: &serde_json::Map<String, Value>) -> Vec<Fact> {
    deps.iter()
        .filter_map(|(name, value)| {
            let version = value.get("version")?.as_str()?.to_string();
            Some(Fact::Dependency {
                name: name.clone(),
                version,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/lockfile")
            .join(name)
    }

    #[test]
    fn parse_v1_lockfile() {
        let facts = parse(&fixture("v1.json")).unwrap();
        assert_eq!(facts.len(), 2);
        assert!(facts.iter().any(
            |f| matches!(f, Fact::Dependency { name, version } if name == "express" && version == "4.18.2")
        ));
        assert!(facts.iter().any(
            |f| matches!(f, Fact::Dependency { name, version } if name == "lodash" && version == "4.17.21")
        ));
    }

    #[test]
    fn parse_v2_prefers_packages_field() {
        let facts = parse(&fixture("v2.json")).unwrap();
        assert_eq!(facts.len(), 2); // top-level only, not nested
        assert!(facts
            .iter()
            .any(|f| matches!(f, Fact::Dependency { name, .. } if name == "express")));
    }

    #[test]
    fn parse_v3_top_level_only() {
        let facts = parse(&fixture("v3.json")).unwrap();
        // body-parser is nested under express, should not appear
        assert_eq!(facts.len(), 2);
        assert!(!facts
            .iter()
            .any(|f| matches!(f, Fact::Dependency { name, .. } if name == "body-parser")));
    }

    #[test]
    fn missing_file_returns_error() {
        assert!(parse(Path::new("/nonexistent/package-lock.json")).is_err());
    }
}
