#[path = "common/mod.rs"]
mod common;

use common::{parse_json, run_fallow};

// ---------------------------------------------------------------------------
// fix --dry-run
// ---------------------------------------------------------------------------

#[test]
fn fix_dry_run_exits_0() {
    let output = run_fallow(
        "fix",
        "basic-project",
        &["--dry-run", "--format", "json", "--quiet"],
    );
    assert_eq!(
        output.code, 0,
        "fix --dry-run should exit 0, stderr: {}",
        output.stderr
    );
}

#[test]
fn fix_dry_run_json_has_dry_run_flag() {
    let output = run_fallow(
        "fix",
        "basic-project",
        &["--dry-run", "--format", "json", "--quiet"],
    );
    let json = parse_json(&output);
    assert_eq!(
        json["dry_run"].as_bool(),
        Some(true),
        "dry_run should be true"
    );
}

#[test]
fn fix_dry_run_finds_fixable_items() {
    let output = run_fallow(
        "fix",
        "basic-project",
        &["--dry-run", "--format", "json", "--quiet"],
    );
    let json = parse_json(&output);
    let fixes = json["fixes"].as_array().unwrap();
    assert!(!fixes.is_empty(), "basic-project should have fixable items");

    // Each fix should have a type
    for fix in fixes {
        assert!(fix.get("type").is_some(), "fix should have 'type'");
        // Export fixes have "path", dependency fixes have "package"
        let has_path = fix.get("path").is_some() || fix.get("package").is_some();
        assert!(has_path, "fix should have 'path' or 'package'");
    }
}

#[test]
fn fix_dry_run_does_not_have_applied_key() {
    let output = run_fallow(
        "fix",
        "basic-project",
        &["--dry-run", "--format", "json", "--quiet"],
    );
    let json = parse_json(&output);
    let fixes = json["fixes"].as_array().unwrap();
    for fix in fixes {
        assert!(
            fix.get("applied").is_none(),
            "dry-run fixes should not have 'applied' key"
        );
    }
}

// ---------------------------------------------------------------------------
// fix without --yes in non-TTY
// ---------------------------------------------------------------------------

#[test]
fn fix_without_yes_in_non_tty_exits_2() {
    // Running fix without --dry-run and without --yes in a non-TTY (test runner)
    // should exit 2 with an error
    let output = run_fallow("fix", "basic-project", &["--format", "json", "--quiet"]);
    assert_eq!(output.code, 2, "fix without --yes in non-TTY should exit 2");
}
