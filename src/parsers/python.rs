use crate::model::{Fact, Runtime};
use std::path::Path;

pub fn parse_python_version(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let version = content.trim().to_string();
    if version.is_empty() {
        return Ok(vec![]);
    }
    Ok(vec![Fact::RuntimeVersion {
        runtime: Runtime::Python,
        version,
        source: path.display().to_string(),
    }])
}

pub fn parse_runtime_txt(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let version = content
        .trim()
        .strip_prefix("python-")
        .unwrap_or("")
        .to_string();
    if version.is_empty() {
        return Ok(vec![]);
    }
    Ok(vec![Fact::RuntimeVersion {
        runtime: Runtime::Python,
        version,
        source: path.display().to_string(),
    }])
}

pub fn parse_pyproject_toml(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let parsed: toml::Value = content.parse()?;

    let requires = parsed
        .get("project")
        .and_then(|p| p.get("requires-python"))
        .and_then(|r| r.as_str());

    let Some(specifier) = requires else {
        return Ok(vec![]);
    };

    // Extract first version number (major.minor) from specifier like ">=3.10"
    let version: String = specifier
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();

    if version.is_empty() {
        return Ok(vec![]);
    }

    Ok(vec![Fact::RuntimeVersion {
        runtime: Runtime::Python,
        version,
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
            .join("tests/fixtures/python")
            .join(name)
    }

    #[test]
    fn python_version_file() {
        let facts = parse_python_version(&fixture("python-version")).unwrap();
        assert!(
            matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::Python, version, .. } if version == "3.12.1")
        );
    }

    #[test]
    fn runtime_txt() {
        let facts = parse_runtime_txt(&fixture("runtime.txt")).unwrap();
        assert!(
            matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::Python, version, .. } if version == "3.11.6")
        );
    }

    #[test]
    fn pyproject_toml() {
        let facts = parse_pyproject_toml(&fixture("pyproject.toml")).unwrap();
        assert!(
            matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::Python, version, .. } if version == "3.10")
        );
    }
}
