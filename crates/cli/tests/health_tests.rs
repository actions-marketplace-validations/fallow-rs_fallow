#[path = "common/mod.rs"]
mod common;

use common::{fixture_path, parse_json, redact_all, run_fallow, run_fallow_in_root};
use std::path::Path;
use tempfile::tempdir;

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent directories");
    }
    std::fs::write(path, contents).expect("write file");
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create destination directory");
    for entry in std::fs::read_dir(src).expect("read source directory") {
        let entry = entry.expect("read source entry");
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let file_type = entry.file_type().expect("read source entry type");
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
        } else if !file_type.is_dir() {
            std::fs::copy(&src_path, &dst_path).expect("copy file");
        }
    }
}

fn git(root: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        // Isolate from parent git context (pre-push hook sets GIT_DIR to the main repo,
        // which overrides current_dir and causes commits to leak into the real repo)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} should succeed");
}

// ---------------------------------------------------------------------------
// JSON output structure
// ---------------------------------------------------------------------------

#[test]
fn health_json_output_is_valid() {
    // Disable the default CRAP gate (30.0) so the fixture's branchy untested
    // function doesn't push the process to exit 1. This test only verifies
    // shape, not findings.
    let output = run_fallow(
        "health",
        "complexity-project",
        &["--max-crap", "10000", "--format", "json", "--quiet"],
    );
    assert_eq!(output.code, 0, "health should succeed");
    let json = parse_json(&output);
    assert!(json.is_object(), "health JSON output should be an object");
}

#[test]
fn health_json_has_findings() {
    let output = run_fallow(
        "health",
        "complexity-project",
        &["--complexity", "--format", "json", "--quiet"],
    );
    let json = parse_json(&output);
    // complexity-project has a function with cyclomatic > 10
    assert!(
        json.get("findings").is_some(),
        "health JSON should have findings key"
    );
}

#[test]
fn health_save_baseline_creates_parent_directory() {
    let dir = tempdir().unwrap();
    write_file(
        &dir.path().join("package.json"),
        r#"{"name":"health-save","version":"1.0.0"}"#,
    );
    write_file(
        &dir.path().join("src/index.ts"),
        r"export function alpha(value: number): number {
  if (value > 10) return value * 2;
  return value + 1;
}
",
    );

    let baseline_path = dir.path().join("fallow-baselines/health.json");
    let output = run_fallow_in_root(
        "health",
        dir.path(),
        &[
            "--targets",
            "--save-baseline",
            baseline_path.to_str().unwrap(),
            "--format",
            "json",
            "--quiet",
        ],
    );
    let rendered = redact_all(&format!("{}\n{}", output.stdout, output.stderr), dir.path());
    assert_eq!(
        output.code, 0,
        "health save baseline should succeed: {rendered}"
    );
    assert!(
        baseline_path.exists(),
        "health save baseline should create nested file: {rendered}"
    );
}

// ---------------------------------------------------------------------------
// Exit code with threshold
// ---------------------------------------------------------------------------

#[test]
fn health_exits_0_below_threshold() {
    let output = run_fallow(
        "health",
        "complexity-project",
        &[
            "--max-cyclomatic",
            "50",
            // Raise the CRAP gate out of the way so this test isolates the
            // cyclomatic/cognitive behaviour under test.
            "--max-crap",
            "10000",
            "--complexity",
            "--format",
            "json",
            "--quiet",
        ],
    );
    assert_eq!(
        output.code, 0,
        "health should exit 0 when complexity below threshold"
    );
}

#[test]
fn health_exits_1_when_threshold_exceeded() {
    let output = run_fallow(
        "health",
        "complexity-project",
        &[
            "--max-cyclomatic",
            "3",
            "--complexity",
            "--fail-on-issues",
            "--format",
            "json",
            "--quiet",
        ],
    );
    assert_eq!(
        output.code, 1,
        "health should exit 1 when complexity exceeds threshold"
    );
}

// ---------------------------------------------------------------------------
// CRAP threshold (--max-crap)
// ---------------------------------------------------------------------------

/// With a high `--max-crap`, no function should trigger a CRAP finding and the
/// summary's `max_crap_threshold` must reflect the CLI override.
#[test]
fn health_exits_0_when_crap_below_threshold() {
    let output = run_fallow(
        "health",
        "complexity-project",
        &[
            "--max-cyclomatic",
            "99",
            "--max-crap",
            "10000",
            "--complexity",
            "--format",
            "json",
            "--quiet",
        ],
    );
    assert_eq!(
        output.code, 0,
        "health should exit 0 when CRAP stays below a very high threshold"
    );
    let json: serde_json::Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(
        json["summary"]["max_crap_threshold"].as_f64(),
        Some(10_000.0),
        "summary should echo the CLI-supplied threshold"
    );
}

/// With a very low `--max-crap`, every nontrivial function should become a
/// finding and the command must exit 1.
#[test]
fn health_exits_1_when_crap_threshold_exceeded() {
    let output = run_fallow(
        "health",
        "complexity-project",
        &[
            "--max-cyclomatic",
            "9999",
            "--max-cognitive",
            "9999",
            "--max-crap",
            "1",
            "--complexity",
            "--fail-on-issues",
            "--format",
            "json",
            "--quiet",
        ],
    );
    assert_eq!(
        output.code, 1,
        "health should exit 1 when any function has CRAP >= 1"
    );
    let json: serde_json::Value = serde_json::from_str(&output.stdout).unwrap();
    let findings = json["findings"].as_array().expect("findings array");
    assert!(
        !findings.is_empty(),
        "crap-triggered run should emit at least one finding"
    );
    let any_crap = findings
        .iter()
        .any(|f| f.get("crap").and_then(|v| v.as_f64()).is_some());
    assert!(
        any_crap,
        "at least one finding should carry a populated `crap` score when --max-crap triggered"
    );
}

// ---------------------------------------------------------------------------
// Section flags
// ---------------------------------------------------------------------------

#[test]
fn health_score_flag_shows_score() {
    let output = run_fallow(
        "health",
        "complexity-project",
        &["--score", "--format", "json", "--quiet"],
    );
    let json = parse_json(&output);
    assert!(
        json.get("score").is_some() || json.get("health_score").is_some(),
        "health --score should include score data"
    );
    assert!(
        json.get("file_scores").is_none(),
        "health --score should not render file_scores"
    );
    assert!(
        json.get("coverage_gaps").is_none(),
        "health --score should not render coverage_gaps"
    );
    assert!(
        json.get("hotspot_summary").is_none(),
        "health --score should not render hotspot summaries"
    );
    assert!(
        json.get("vital_signs").is_none(),
        "health --score should not render vital signs"
    );
}

#[test]
fn health_score_flag_with_config_does_not_render_coverage_gaps() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let config_path = dir.path().join("fallow.json");
    write_file(
        &config_path,
        r#"{
  "rules": {
    "coverage-gaps": "warn"
  }
}"#,
    );

    let root = fixture_path("production-mode");
    let output = common::run_fallow_in_root(
        "health",
        &root,
        &[
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--score",
            "--format",
            "json",
            "--quiet",
        ],
    );
    assert_eq!(output.code, 0, "health --score should still succeed");

    let json = parse_json(&output);
    assert!(
        json.get("coverage_gaps").is_none(),
        "config-enabled coverage gaps should not override explicit section selection"
    );
}

#[test]
fn health_baseline_partial_overflow_does_not_emit_stale_baseline_warning() {
    let dir = tempfile::tempdir().expect("create temp dir");
    write_file(
        &dir.path().join("package.json"),
        r#"{"name":"baseline-health-repro","type":"module"}"#,
    );
    write_file(
        &dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"ES2020","module":"ES2020","strict":true},"include":["src"]}"#,
    );
    write_file(
        &dir.path().join("src/index.ts"),
        r#"export function alpha(items: number[]): string {
  let result = "";
  for (let i = 0; i < items.length; i++) {
    if (items[i] % 2 === 0) {
      if (items[i] % 3 === 0) {
        if (items[i] % 5 === 0) { result += "fizzbuzz"; }
        else { result += "fizz"; }
      } else if (items[i] % 5 === 0) { result += "buzz"; }
      else { result += String(items[i]); }
    } else {
      if (items[i] % 7 === 0) { result += "lucky"; }
      else if (items[i] > 50) {
        if (items[i] < 75) { result += "mid"; }
        else { result += "high"; }
      } else { result += "low"; }
    }
  }
  return result;
}"#,
    );

    let baseline_path = dir.path().join("health-baseline.json");
    let baseline_path_str = baseline_path
        .to_str()
        .expect("baseline path should be valid UTF-8");

    let save = run_fallow_in_root(
        "health",
        dir.path(),
        &[
            "--complexity",
            "--max-cyclomatic",
            "3",
            "--max-cognitive",
            "3",
            "--save-baseline",
            baseline_path_str,
        ],
    );
    let save_output = redact_all(&format!("{}\n{}", save.stdout, save.stderr), dir.path());
    assert!(
        save.code == 0 || save.code == 1,
        "save baseline should not crash: {save_output}"
    );
    assert!(
        baseline_path.exists(),
        "save baseline should create the baseline file: {save_output}"
    );
    assert!(
        save_output.contains("Saved health baseline to"),
        "save baseline should confirm the write: {save_output}"
    );

    write_file(
        &dir.path().join("src/index.ts"),
        r#"export function alpha(items: number[]): string {
  let result = "";
  for (let i = 0; i < items.length; i++) {
    if (items[i] % 2 === 0) {
      if (items[i] % 3 === 0) {
        if (items[i] % 5 === 0) { result += "fizzbuzz"; }
        else { result += "fizz"; }
      } else if (items[i] % 5 === 0) { result += "buzz"; }
      else { result += String(items[i]); }
    } else {
      if (items[i] % 7 === 0) { result += "lucky"; }
      else if (items[i] > 50) {
        if (items[i] < 75) { result += "mid"; }
        else { result += "high"; }
      } else { result += "low"; }
    }
  }
  return result;
}

export function beta(items: number[]): string {
  let result = "";
  for (let i = 0; i < items.length; i++) {
    if (items[i] % 2 === 0) {
      if (items[i] % 3 === 0) {
        if (items[i] % 5 === 0) { result += "fizzbuzz"; }
        else { result += "fizz"; }
      } else if (items[i] % 5 === 0) { result += "buzz"; }
      else { result += String(items[i]); }
    } else {
      if (items[i] % 7 === 0) { result += "lucky"; }
      else if (items[i] > 50) {
        if (items[i] < 75) { result += "mid"; }
        else { result += "high"; }
      } else { result += "low"; }
    }
  }
  return result;
}"#,
    );

    let load = run_fallow_in_root(
        "health",
        dir.path(),
        &[
            "--complexity",
            "--max-cyclomatic",
            "3",
            "--max-cognitive",
            "3",
            "--baseline",
            baseline_path_str,
        ],
    );
    let combined = redact_all(&format!("{}\n{}", load.stdout, load.stderr), dir.path());
    assert_eq!(
        load.code, 1,
        "baseline load should still report the overflowing findings: {combined}"
    );
    assert!(
        combined.contains("alpha") && combined.contains("beta"),
        "expected overflow run to still report both functions: {combined}"
    );
    assert!(
        !combined.contains("Warning: health baseline has"),
        "partial-overflow baseline should not look stale: {combined}"
    );
}

#[test]
fn health_score_flag_with_config_error_fails_without_rendering_coverage_gaps() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let config_path = dir.path().join("fallow.json");
    write_file(
        &config_path,
        r#"{
  "rules": {
    "coverage-gaps": "error"
  }
}
"#,
    );

    let root = fixture_path("production-mode");
    let output = common::run_fallow_in_root(
        "health",
        &root,
        &[
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--score",
            "--format",
            "json",
            "--quiet",
        ],
    );
    assert_eq!(
        output.code, 1,
        "coverage-gaps=error should still fail score-only health runs"
    );

    let json = parse_json(&output);
    assert!(
        json.get("coverage_gaps").is_none(),
        "gate-only coverage gaps should not be rendered in score-only output"
    );
}

#[test]
fn health_file_scores_flag() {
    let output = run_fallow(
        "health",
        "complexity-project",
        &["--file-scores", "--format", "json", "--quiet"],
    );
    let json = parse_json(&output);
    assert!(
        json.get("file_scores").is_some(),
        "health --file-scores should include file_scores"
    );
}

#[test]
fn health_file_scores_include_vue_sfc_files() {
    let output = run_fallow(
        "health",
        "vue-split-type-value-export",
        &["--file-scores", "--format", "json", "--quiet"],
    );
    assert_eq!(output.code, 0, "health should score Vue SFC files");

    let json = parse_json(&output);
    let file_scores = json["file_scores"]
        .as_array()
        .expect("health --file-scores should include file_scores");

    assert!(
        file_scores.iter().any(|score| {
            score.get("path").and_then(serde_json::Value::as_str) == Some("src/App.vue")
        }),
        "Vue SFC files should be included in file_scores: {file_scores:?}"
    );
}

#[test]
fn health_complexity_reports_vue_sfc_functions() {
    let output = run_fallow(
        "health",
        "vue-split-type-value-export",
        &[
            "--complexity",
            "--max-cyclomatic",
            "0",
            "--max-crap",
            "10000",
            "--format",
            "json",
            "--quiet",
        ],
    );
    assert_eq!(
        output.code, 1,
        "health should report Vue SFC complexity findings"
    );

    let json = parse_json(&output);
    let findings = json["findings"]
        .as_array()
        .expect("health --complexity should include findings");

    assert!(
        findings.iter().any(|finding| {
            finding.get("path").and_then(serde_json::Value::as_str) == Some("src/App.vue")
                && finding.get("name").and_then(serde_json::Value::as_str) == Some("isStatus")
        }),
        "Vue SFC functions should surface as health findings: {findings:?}"
    );
}

#[test]
fn health_coverage_gaps_flag_reports_runtime_gaps() {
    let output = run_fallow(
        "health",
        "coverage-gaps",
        &["--coverage-gaps", "--format", "json", "--quiet"],
    );
    assert_eq!(
        output.code, 0,
        "health --coverage-gaps defaults to warn severity (exit 0)"
    );

    let json = parse_json(&output);
    let coverage = json
        .get("coverage_gaps")
        .expect("health --coverage-gaps should include coverage_gaps");
    let files = coverage["files"]
        .as_array()
        .expect("coverage_gaps.files should be an array");
    let exports = coverage["exports"]
        .as_array()
        .expect("coverage_gaps.exports should be an array");

    let file_names: Vec<String> = files
        .iter()
        .filter_map(|item| item.get("path").and_then(serde_json::Value::as_str))
        .map(|p| p.replace('\\', "/"))
        .collect();
    assert!(
        file_names
            .iter()
            .any(|path| path.ends_with("src/setup-only.ts")),
        "setup-only.ts should remain untested even when referenced by test setup: {file_names:?}"
    );
    assert!(
        file_names
            .iter()
            .any(|path| path.ends_with("src/fixture-only.ts")),
        "fixture-only.ts should remain untested even when referenced by a fixture: {file_names:?}"
    );
    assert!(
        !file_names
            .iter()
            .any(|path| path.ends_with("src/covered.ts")),
        "covered.ts should not be reported as an untested file: {file_names:?}"
    );

    let export_names: Vec<_> = exports
        .iter()
        .filter_map(|item| item.get("export_name").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        !export_names.contains(&"covered"),
        "covered should not be reported as an untested export: {export_names:?}"
    );
    assert!(
        !export_names.contains(&"indirectlyCovered"),
        "exports already reported as dead code should be excluded from coverage gaps: {export_names:?}"
    );
}

#[test]
fn health_coverage_gaps_config_error_enforces_without_flag() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let config_path = dir.path().join("fallow.json");
    write_file(
        &config_path,
        r#"{
  "rules": {
    "coverage-gaps": "error"
  }
}
"#,
    );

    let root = fixture_path("production-mode");
    let output = common::run_fallow_in_root(
        "health",
        &root,
        &[
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--format",
            "json",
            "--quiet",
        ],
    );
    assert_eq!(
        output.code, 1,
        "coverage-gaps=error should fail health even without --coverage-gaps"
    );

    let json = parse_json(&output);
    assert!(
        json.get("coverage_gaps").is_some(),
        "config-enabled coverage gaps should be present in the report"
    );
}

#[test]
fn health_coverage_gaps_production_excludes_dead_test_helpers() {
    let output = run_fallow(
        "health",
        "production-mode",
        &[
            "--production",
            "--coverage-gaps",
            "--format",
            "json",
            "--quiet",
        ],
    );
    assert_eq!(
        output.code, 0,
        "production coverage gaps default to warn severity (exit 0)"
    );

    let json = parse_json(&output);
    let coverage = json["coverage_gaps"]
        .as_object()
        .expect("production coverage_gaps should be an object");

    let export_names: Vec<_> = coverage["exports"]
        .as_array()
        .expect("coverage_gaps.exports should be an array")
        .iter()
        .filter_map(|item| item.get("export_name").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        !export_names.contains(&"testHelper"),
        "exports already reported as dead code should not also be reported as coverage gaps: {export_names:?}"
    );
    assert!(
        export_names.contains(&"app") && export_names.contains(&"helper"),
        "production coverage gaps should still report runtime exports lacking test reachability: {export_names:?}"
    );

    let summary = coverage["summary"]
        .as_object()
        .expect("coverage_gaps.summary should be an object");
    assert_eq!(
        summary["untested_exports"].as_u64(),
        Some(2),
        "production coverage gaps should exclude dead exports from the export count"
    );
}

#[test]
fn health_coverage_gaps_suppressed_file_excluded() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let root = dir.path();
    copy_dir_recursive(&fixture_path("coverage-gaps"), root);

    // Add suppression comment to setup-only.ts
    write_file(
        &root.join("src/setup-only.ts"),
        r#"// fallow-ignore-file coverage-gaps
export function viaSetup(): string {
  return "setup";
}
"#,
    );

    let output = common::run_fallow_in_root(
        "health",
        root,
        &["--coverage-gaps", "--format", "json", "--quiet"],
    );

    let json = parse_json(&output);
    let coverage = json
        .get("coverage_gaps")
        .expect("coverage_gaps should be present");
    let file_paths: Vec<String> = coverage["files"]
        .as_array()
        .expect("files array")
        .iter()
        .filter_map(|item| item.get("path").and_then(serde_json::Value::as_str))
        .map(|p| p.replace('\\', "/"))
        .collect();

    assert!(
        !file_paths
            .iter()
            .any(|path| path.ends_with("src/setup-only.ts")),
        "setup-only.ts should be excluded when suppressed with fallow-ignore-file: {file_paths:?}"
    );

    let export_names: Vec<_> = coverage["exports"]
        .as_array()
        .expect("exports array")
        .iter()
        .filter_map(|item| item.get("export_name").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        !export_names.contains(&"viaSetup"),
        "viaSetup export should be excluded when file is suppressed: {export_names:?}"
    );
}

#[test]
fn health_coverage_gaps_workspace_scope_limits_results() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let root = dir.path();

    write_file(
        &root.join("package.json"),
        r#"{
  "name": "coverage-gaps-workspace",
  "private": true,
  "workspaces": ["packages/*"],
  "dependencies": {
    "vitest": "^3.2.4"
  }
}"#,
    );

    write_file(
        &root.join("packages/app/package.json"),
        r#"{
  "name": "app",
  "main": "src/main.ts"
}"#,
    );
    write_file(
        &root.join("packages/app/src/main.ts"),
        r#"import { covered } from "./covered";
import { appGap } from "./app-gap";

export const app = `${covered()}:${appGap()}`;
"#,
    );
    write_file(
        &root.join("packages/app/src/covered.ts"),
        r#"export function covered(): string {
  return "covered";
}
"#,
    );
    write_file(
        &root.join("packages/app/src/app-gap.ts"),
        r#"export function appGap(): string {
  return "app-gap";
}
"#,
    );
    write_file(
        &root.join("packages/app/tests/covered.test.ts"),
        r#"import { describe, expect, it } from "vitest";
import { covered } from "../src/covered";

describe("covered", () => {
  it("covers app runtime code selectively", () => {
    expect(covered()).toBe("covered");
  });
});
"#,
    );

    write_file(
        &root.join("packages/shared/package.json"),
        r#"{
  "name": "shared",
  "main": "src/index.ts"
}"#,
    );
    write_file(
        &root.join("packages/shared/src/index.ts"),
        r#"import { sharedGap } from "./shared-gap";

export const shared = sharedGap();
"#,
    );
    write_file(
        &root.join("packages/shared/src/shared-gap.ts"),
        r#"export function sharedGap(): string {
  return "shared-gap";
}
"#,
    );

    let output = common::run_fallow_in_root(
        "health",
        root,
        &[
            "--coverage-gaps",
            "--workspace",
            "app",
            "--format",
            "json",
            "--quiet",
        ],
    );
    assert_eq!(
        output.code, 0,
        "workspace-scoped health --coverage-gaps defaults to warn severity (exit 0)"
    );

    let json = parse_json(&output);
    let coverage = json["coverage_gaps"]
        .as_object()
        .expect("workspace-scoped coverage_gaps should be an object");

    let file_paths: Vec<String> = coverage["files"]
        .as_array()
        .expect("coverage_gaps.files should be an array")
        .iter()
        .filter_map(|item| item.get("path").and_then(serde_json::Value::as_str))
        .map(|p| p.replace('\\', "/"))
        .collect();
    assert!(
        file_paths.iter().all(|path| path.contains("packages/app/")),
        "workspace scope should only report app package files: {file_paths:?}"
    );
    assert!(
        file_paths
            .iter()
            .any(|path| path.ends_with("packages/app/src/app-gap.ts")),
        "app gap should be reported in workspace scope: {file_paths:?}"
    );
    assert!(
        !file_paths
            .iter()
            .any(|path| path.contains("packages/shared")),
        "shared package gaps should be excluded from app workspace scope: {file_paths:?}"
    );
}

#[test]
fn health_coverage_gaps_changed_since_scopes_results() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let root = dir.path();
    copy_dir_recursive(&fixture_path("coverage-gaps"), root);

    git(root, &["init"]);
    git(root, &["config", "user.name", "Test User"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "initial"]);

    write_file(
        &root.join("src/fixture-only.ts"),
        r#"export function viaFixture(): string {
  return "fixture-only-updated";
}
"#,
    );
    git(root, &["add", "src/fixture-only.ts"]);
    git(root, &["commit", "-m", "update fixture gap"]);

    let output = common::run_fallow_in_root(
        "health",
        root,
        &[
            "--coverage-gaps",
            "--changed-since",
            "HEAD~1",
            "--format",
            "json",
            "--quiet",
        ],
    );
    assert_eq!(
        output.code, 0,
        "changed-since coverage gaps defaults to warn severity (exit 0)"
    );

    let json = parse_json(&output);
    let coverage = json["coverage_gaps"]
        .as_object()
        .expect("changed-since coverage_gaps should be an object");

    let file_paths: Vec<String> = coverage["files"]
        .as_array()
        .expect("coverage_gaps.files should be an array")
        .iter()
        .filter_map(|item| item.get("path").and_then(serde_json::Value::as_str))
        .map(|p| p.replace('\\', "/"))
        .collect();
    assert_eq!(
        file_paths.len(),
        1,
        "changed-since should limit file gaps to changed files: {file_paths:?}"
    );
    assert!(
        file_paths[0].ends_with("src/fixture-only.ts"),
        "changed-since should report the changed fixture-only file, got: {file_paths:?}"
    );

    let summary = coverage["summary"]
        .as_object()
        .expect("coverage_gaps.summary should be an object");
    assert_eq!(
        summary["runtime_files"].as_u64(),
        Some(1),
        "changed-since should recompute runtime scope summary for changed files only"
    );
}

// ---------------------------------------------------------------------------
// Human output snapshot
// ---------------------------------------------------------------------------

#[test]
fn health_human_output_snapshot() {
    // Use --max-cyclomatic 10 so the 14-branch classify() function exceeds the threshold
    // and produces actual output to snapshot (default threshold of 20 would show nothing)
    let output = run_fallow(
        "health",
        "complexity-project",
        &["--complexity", "--max-cyclomatic", "10", "--quiet"],
    );
    let root = fixture_path("complexity-project");
    let redacted = redact_all(&output.stdout, &root);
    insta::assert_snapshot!("health_human_complexity", redacted);
}
