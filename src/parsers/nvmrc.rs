use crate::model::{Fact, Runtime};
use std::path::Path;

pub fn parse_nvmrc(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let version = content
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .unwrap_or("");

    let version = version.strip_prefix('v').unwrap_or(version);

    // Check if it's a numeric version (starts with a digit)
    if version.is_empty() || !version.starts_with(|c: char| c.is_ascii_digit()) {
        // Alias like lts/iron, node, stable — skip
        return Ok(vec![]);
    }

    Ok(vec![Fact::RuntimeVersion {
        runtime: Runtime::NodeJs,
        version: version.to_string(),
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
            .join("tests/fixtures/nvmrc")
            .join(name)
    }

    #[test]
    fn pinned_version() {
        let facts = parse_nvmrc(&fixture("pinned")).unwrap();
        assert_eq!(facts.len(), 1);
        assert!(
            matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::NodeJs, version, .. } if version == "20.11.0")
        );
    }

    #[test]
    fn major_only() {
        let facts = parse_nvmrc(&fixture("major_only")).unwrap();
        assert!(matches!(&facts[0], Fact::RuntimeVersion { version, .. } if version == "20"));
    }

    #[test]
    fn strips_v_prefix() {
        let facts = parse_nvmrc(&fixture("v_prefix")).unwrap();
        assert!(matches!(&facts[0], Fact::RuntimeVersion { version, .. } if version == "20.11.0"));
    }

    #[test]
    fn alias_returns_empty() {
        let facts = parse_nvmrc(&fixture("lts_alias")).unwrap();
        assert!(facts.is_empty());
    }

    #[test]
    fn ignores_comments() {
        let facts = parse_nvmrc(&fixture("with_comments")).unwrap();
        assert!(matches!(&facts[0], Fact::RuntimeVersion { version, .. } if version == "18.19.0"));
    }
}
