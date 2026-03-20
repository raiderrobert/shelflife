pub mod java;
pub mod npm_lockfile;
pub mod nvmrc;
pub mod package_json;
pub mod python;

use crate::model::Fact;
use std::path::Path;

type ParserFn = fn(&Path) -> Result<Vec<Fact>, Box<dyn std::error::Error>>;

/// Discover known files in the directory and run all applicable parsers.
pub fn parse_directory(dir: &Path) -> Vec<Fact> {
    let mut facts = Vec::new();

    let try_parse = |file: &str, parser: ParserFn| -> Vec<Fact> {
        let path = dir.join(file);
        if path.exists() {
            parser(&path).unwrap_or_default()
        } else {
            vec![]
        }
    };

    facts.extend(try_parse("package-lock.json", npm_lockfile::parse));
    facts.extend(try_parse(".nvmrc", nvmrc::parse_nvmrc));
    facts.extend(try_parse(".node-version", nvmrc::parse_nvmrc));
    facts.extend(try_parse(
        "package.json",
        package_json::parse_package_json_engines,
    ));
    facts.extend(try_parse(".python-version", python::parse_python_version));
    facts.extend(try_parse("runtime.txt", python::parse_runtime_txt));
    facts.extend(try_parse("pyproject.toml", python::parse_pyproject_toml));
    facts.extend(try_parse("pom.xml", java::parse_pom_xml));
    facts.extend(try_parse("build.gradle", java::parse_build_gradle));

    facts
}
