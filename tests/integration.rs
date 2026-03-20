use std::path::PathBuf;
use std::process::Command;

fn bin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_shelflife"))
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/integration")
        .join(name)
}

#[test]
fn help_flag_works() {
    let output = Command::new(bin_path())
        .arg("--help")
        .output()
        .expect("failed to run shelflife");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("shelflife") || stdout.contains("Dependency freshness checker"));
}

#[test]
fn version_flag_works() {
    let output = Command::new(bin_path())
        .arg("--version")
        .output()
        .expect("failed to run shelflife");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0.1.0"));
}

#[test]
fn fail_on_none_always_exits_zero() {
    let project = fixture_path("clean_project");
    let output = Command::new(bin_path())
        .arg(project)
        .arg("--fail-on")
        .arg("none")
        .output()
        .expect("failed to run shelflife");

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit code 0 with --fail-on none, got: {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn json_output_is_valid() {
    let project = fixture_path("clean_project");
    let output = Command::new(bin_path())
        .arg(project)
        .arg("--json")
        .arg("--fail-on")
        .arg("none")
        .output()
        .expect("failed to run shelflife");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(
        parsed.is_ok(),
        "stdout is not valid JSON:\n{stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json = parsed.unwrap();
    assert!(json.get("findings").is_some(), "missing 'findings' key");
    assert!(json.get("counts").is_some(), "missing 'counts' key");
}
