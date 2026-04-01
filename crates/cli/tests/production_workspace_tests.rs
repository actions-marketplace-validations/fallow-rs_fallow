#[path = "common/mod.rs"]
mod common;

use common::{parse_json, run_fallow};

// ---------------------------------------------------------------------------
// --production mode
// ---------------------------------------------------------------------------

#[test]
fn production_mode_check_exits_successfully() {
    let output = run_fallow(
        "check",
        "basic-project",
        &["--production", "--format", "json", "--quiet"],
    );
    assert!(
        output.code == 0 || output.code == 1,
        "--production should not crash, got exit {}",
        output.code
    );
    let json = parse_json(&output);
    assert!(
        json.get("total_issues").is_some(),
        "production mode should still produce results"
    );
}

#[test]
fn production_mode_health_exits_successfully() {
    let output = run_fallow(
        "health",
        "basic-project",
        &["--production", "--format", "json", "--quiet"],
    );
    assert!(
        output.code == 0 || output.code == 1,
        "health --production should not crash"
    );
}

#[test]
fn production_mode_dupes_exits_successfully() {
    let output = run_fallow(
        "dupes",
        "basic-project",
        &["--production", "--format", "json", "--quiet"],
    );
    assert!(
        output.code == 0 || output.code == 1,
        "dupes --production should not crash"
    );
}

// ---------------------------------------------------------------------------
// --workspace scoping
// ---------------------------------------------------------------------------

#[test]
fn workspace_scoping_limits_output_to_package() {
    let output = run_fallow(
        "check",
        "workspace-project",
        &["--workspace", "shared", "--format", "json", "--quiet"],
    );
    assert!(
        output.code == 0 || output.code == 1,
        "--workspace should not crash, got exit {}. stderr: {}",
        output.code,
        output.stderr
    );
    let json = parse_json(&output);

    // All reported file paths should be within packages/shared/
    for file in json["unused_files"].as_array().unwrap_or(&Vec::new()) {
        let path = file["path"]
            .as_str()
            .expect("unused_files entry should have 'path' string")
            .replace('\\', "/");
        assert!(
            path.contains("packages/shared/"),
            "workspace-scoped unused file should be in packages/shared/, got: {path}"
        );
    }
    for export in json["unused_exports"].as_array().unwrap_or(&Vec::new()) {
        let path = export["path"]
            .as_str()
            .expect("unused_exports entry should have 'path' string")
            .replace('\\', "/");
        assert!(
            path.contains("packages/shared/"),
            "workspace-scoped unused export should be in packages/shared/, got: {path}"
        );
    }
}

#[test]
fn workspace_scoping_on_nonexistent_package() {
    let output = run_fallow(
        "check",
        "workspace-project",
        &[
            "--workspace",
            "nonexistent-pkg",
            "--format",
            "json",
            "--quiet",
        ],
    );
    // Should either exit 0 with no issues (package not found = nothing scoped)
    // or exit 2 (invalid workspace). Both are acceptable.
    assert!(
        output.code == 0 || output.code == 2,
        "nonexistent workspace should exit 0 or 2, got {}",
        output.code
    );
}

// ---------------------------------------------------------------------------
// --regression-baseline round-trip
// ---------------------------------------------------------------------------

#[test]
fn regression_baseline_round_trip() {
    let dir = std::env::temp_dir().join(format!("fallow-regression-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let baseline_path = dir.join("regression.json");

    // Save regression baseline
    let output = run_fallow(
        "check",
        "basic-project",
        &[
            "--save-regression-baseline",
            baseline_path.to_str().unwrap(),
            "--format",
            "json",
            "--quiet",
        ],
    );
    assert!(
        output.code == 0 || output.code == 1,
        "save-regression-baseline should not crash"
    );
    assert!(
        baseline_path.exists(),
        "--save-regression-baseline should create file"
    );

    // Run with --fail-on-regression against same project — counts unchanged
    // Note: exit code 1 is still possible because check exits 1 on error-severity issues.
    // --fail-on-regression only adds an ADDITIONAL exit-1 if counts increased.
    // The important thing is it doesn't exit 2 (crash) and the regression check passes.
    let output = run_fallow(
        "check",
        "basic-project",
        &[
            "--fail-on-regression",
            "--regression-baseline",
            baseline_path.to_str().unwrap(),
            "--format",
            "json",
            "--quiet",
        ],
    );
    assert!(
        output.code == 0 || output.code == 1,
        "regression check should not crash, got exit {}. stderr: {}",
        output.code,
        output.stderr
    );

    let _ = std::fs::remove_dir_all(&dir);
}
