use crate::model::{Fact, Runtime};
use serde_json::Value;
use std::path::Path;

pub fn parse_package_json_engines(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let parsed: Value = serde_json::from_str(&content)?;

    let node_range = parsed
        .get("engines")
        .and_then(|e| e.get("node"))
        .and_then(|n| n.as_str());

    let Some(range) = node_range else {
        return Ok(vec![]);
    };

    // Extract first numeric sequence as the minimum major version.
    // Handles: ">=18", "^20.0.0", "18.x", "~18.0.0", etc.
    let major: String = range
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();

    if major.is_empty() {
        return Ok(vec![]);
    }

    Ok(vec![Fact::RuntimeVersion {
        runtime: Runtime::NodeJs,
        version: major,
        source: path.display().to_string(),
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fact, Runtime};
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/package_json")
            .join(name)
    }

    #[test]
    fn extracts_minimum_major_from_gte_range() {
        let facts = parse_package_json_engines(&fixture("with_engines.json")).unwrap();
        assert_eq!(facts.len(), 1);
        assert!(
            matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::NodeJs, version, .. } if version == "18")
        );
    }

    #[test]
    fn extracts_major_from_caret_range() {
        let facts = parse_package_json_engines(&fixture("caret_engines.json")).unwrap();
        assert!(matches!(&facts[0], Fact::RuntimeVersion { version, .. } if version == "20"));
    }

    #[test]
    fn no_engines_returns_empty() {
        let facts = parse_package_json_engines(&fixture("no_engines.json")).unwrap();
        assert!(facts.is_empty());
    }
}
