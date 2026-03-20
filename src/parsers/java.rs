use crate::model::{Fact, Runtime};
use std::path::Path;

pub fn parse_pom_xml(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;

    // Try <maven.compiler.source>, <maven.compiler.target>, <maven.compiler.release>, etc.
    let tags = [
        "maven.compiler.source",
        "maven.compiler.target",
        "maven.compiler.release",
        "source",
        "target",
        "release",
    ];

    for tag in tags {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        if let Some(start) = content.find(&open) {
            let value_start = start + open.len();
            if let Some(end) = content[value_start..].find(&close) {
                let value = content[value_start..value_start + end].trim();
                // Skip property references like ${java.version}
                if value.starts_with("${") {
                    continue;
                }
                if value.chars().all(|c| c.is_ascii_digit() || c == '.') && !value.is_empty() {
                    return Ok(vec![Fact::RuntimeVersion {
                        runtime: Runtime::Java,
                        version: value.to_string(),
                        source: path.display().to_string(),
                    }]);
                }
            }
        }
    }

    Ok(vec![])
}

pub fn parse_build_gradle(path: &Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;

    // Match: sourceCompatibility = '17' or "17"
    // Match: sourceCompatibility = JavaVersion.VERSION_17
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("sourceCompatibility") {
            continue;
        }

        // Try JavaVersion.VERSION_XX
        if let Some(pos) = trimmed.find("VERSION_") {
            let version: String = trimmed[pos + 8..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !version.is_empty() {
                return Ok(vec![Fact::RuntimeVersion {
                    runtime: Runtime::Java,
                    version,
                    source: path.display().to_string(),
                }]);
            }
        }

        // Try quoted version: '17' or "17"
        let version: String = trimmed
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if !version.is_empty() {
            return Ok(vec![Fact::RuntimeVersion {
                runtime: Runtime::Java,
                version,
                source: path.display().to_string(),
            }]);
        }
    }

    Ok(vec![])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fact, Runtime};
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/java")
            .join(name)
    }

    #[test]
    fn pom_xml_properties() {
        let facts = parse_pom_xml(&fixture("pom.xml")).unwrap();
        assert!(
            matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::Java, version, .. } if version == "17")
        );
    }

    #[test]
    fn build_gradle_quoted() {
        let facts = parse_build_gradle(&fixture("build.gradle")).unwrap();
        assert!(
            matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::Java, version, .. } if version == "17")
        );
    }

    #[test]
    fn build_gradle_enum() {
        let facts = parse_build_gradle(&fixture("build_enum.gradle")).unwrap();
        assert!(
            matches!(&facts[0], Fact::RuntimeVersion { runtime: Runtime::Java, version, .. } if version == "21")
        );
    }
}
