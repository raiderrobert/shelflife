#![allow(dead_code)]

use crate::model::{Fact, Runtime};
use std::collections::HashMap;

/// Source priority per runtime — index in this list determines priority (lower = higher priority).
fn source_priority(runtime: &Runtime) -> &'static [&'static str] {
    match runtime {
        Runtime::NodeJs => &[".nvmrc", ".node-version", "package.json"],
        Runtime::Python => &[".python-version", "runtime.txt", "pyproject.toml"],
        Runtime::Java => &["pom.xml", "build.gradle"],
    }
}

#[derive(Debug)]
pub struct ResolvedVersion {
    pub version: String,
    pub source: String,
}

#[derive(Debug)]
pub struct ResolvedDependency {
    pub name: String,
    pub version: String,
}

#[derive(Debug)]
pub struct Resolved {
    pub dependencies: Vec<ResolvedDependency>,
    pub runtimes: HashMap<Runtime, ResolvedVersion>,
}

pub fn resolve(facts: Vec<Fact>) -> Resolved {
    let mut dependencies = Vec::new();
    let mut runtime_candidates: HashMap<Runtime, Vec<(String, String)>> = HashMap::new();

    for fact in facts {
        match fact {
            Fact::Dependency { name, version } => {
                dependencies.push(ResolvedDependency { name, version });
            }
            Fact::RuntimeVersion {
                runtime,
                version,
                source,
            } => {
                runtime_candidates
                    .entry(runtime)
                    .or_default()
                    .push((version, source));
            }
        }
    }

    let mut runtimes = HashMap::new();
    for (runtime, candidates) in runtime_candidates {
        let priority = source_priority(&runtime);
        // Pick the candidate whose source matches the earliest entry in the priority list.
        let best = candidates.into_iter().min_by_key(|(_, source)| {
            priority
                .iter()
                .position(|p| source.ends_with(p))
                .unwrap_or(usize::MAX)
        });
        if let Some((version, source)) = best {
            runtimes.insert(runtime, ResolvedVersion { version, source });
        }
    }

    Resolved {
        dependencies,
        runtimes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fact, Runtime};

    #[test]
    fn resolves_dependencies() {
        let facts = vec![
            Fact::Dependency {
                name: "express".into(),
                version: "4.18.2".into(),
            },
            Fact::Dependency {
                name: "lodash".into(),
                version: "4.17.21".into(),
            },
        ];
        let resolved = resolve(facts);
        assert_eq!(resolved.dependencies.len(), 2);
    }

    #[test]
    fn picks_nvmrc_over_engines() {
        let facts = vec![
            Fact::RuntimeVersion {
                runtime: Runtime::NodeJs,
                version: "20".into(),
                source: ".nvmrc".into(),
            },
            Fact::RuntimeVersion {
                runtime: Runtime::NodeJs,
                version: "18".into(),
                source: "package.json".into(),
            },
        ];
        let resolved = resolve(facts);
        let node = resolved.runtimes.get(&Runtime::NodeJs).unwrap();
        assert_eq!(node.version, "20");
    }

    #[test]
    fn picks_python_version_over_pyproject() {
        let facts = vec![
            Fact::RuntimeVersion {
                runtime: Runtime::Python,
                version: "3.12.1".into(),
                source: ".python-version".into(),
            },
            Fact::RuntimeVersion {
                runtime: Runtime::Python,
                version: "3.10".into(),
                source: "pyproject.toml".into(),
            },
        ];
        let resolved = resolve(facts);
        let python = resolved.runtimes.get(&Runtime::Python).unwrap();
        assert_eq!(python.version, "3.12.1");
    }

    #[test]
    fn no_runtime_if_none_detected() {
        let facts = vec![Fact::Dependency {
            name: "express".into(),
            version: "4.18.2".into(),
        }];
        let resolved = resolve(facts);
        assert!(resolved.runtimes.is_empty());
    }

    #[test]
    fn integration_full_project() {
        use crate::parsers::parse_directory;
        use std::path::PathBuf;

        let dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/resolver/full_project");
        let facts = parse_directory(&dir);
        let resolved = resolve(facts);

        assert_eq!(resolved.dependencies.len(), 1);
        assert_eq!(resolved.dependencies[0].name, "express");

        let node = resolved.runtimes.get(&Runtime::NodeJs).unwrap();
        assert_eq!(node.version, "20.11.0"); // .nvmrc wins over engines.node

        let python = resolved.runtimes.get(&Runtime::Python).unwrap();
        assert_eq!(python.version, "3.12.1");
    }
}
