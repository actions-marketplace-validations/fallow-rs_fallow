#[path = "common/mod.rs"]
mod common;

use common::run_fallow_raw;
use std::fs;

/// Create a temp dir with a knip config for migration testing.
fn migrate_temp_dir(suffix: &str, config_name: &str, config_content: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fallow-migrate-test-{}-{}",
        std::process::id(),
        suffix
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("package.json"),
        r#"{"name": "migrate-test", "main": "src/index.ts"}"#,
    )
    .unwrap();
    fs::write(dir.join(config_name), config_content).unwrap();
    dir
}

fn cleanup(dir: &std::path::Path) {
    let _ = fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// Migrate dry-run
// ---------------------------------------------------------------------------

#[test]
fn migrate_dry_run_outputs_config() {
    let dir = migrate_temp_dir(
        "dryrun",
        "knip.json",
        r#"{"entry": ["src/index.ts"], "ignore": ["dist/**"]}"#,
    );
    let output = run_fallow_raw(&[
        "migrate",
        "--dry-run",
        "--root",
        dir.to_str().unwrap(),
        "--quiet",
    ]);
    assert_eq!(
        output.code, 0,
        "migrate --dry-run should exit 0, stderr: {}",
        output.stderr
    );
    // Dry-run prints the generated config to stdout
    assert!(
        output.stdout.contains("entry") || output.stdout.contains("$schema"),
        "dry-run should output the migrated config"
    );
    cleanup(&dir);
}

#[test]
fn migrate_dry_run_toml_output() {
    let dir = migrate_temp_dir("toml", "knip.json", r#"{"entry": ["src/index.ts"]}"#);
    let output = run_fallow_raw(&[
        "migrate",
        "--dry-run",
        "--toml",
        "--root",
        dir.to_str().unwrap(),
        "--quiet",
    ]);
    assert_eq!(output.code, 0, "migrate --dry-run --toml should exit 0");
    // TOML output should use = syntax
    assert!(
        output.stdout.contains('='),
        "TOML output should use = syntax"
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// Migrate error handling
// ---------------------------------------------------------------------------

#[test]
fn migrate_no_config_exits_2() {
    let dir = std::env::temp_dir().join(format!("fallow-migrate-noconfig-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("package.json"), r#"{"name": "no-config"}"#).unwrap();

    let output = run_fallow_raw(&[
        "migrate",
        "--dry-run",
        "--root",
        dir.to_str().unwrap(),
        "--quiet",
    ]);
    assert_eq!(
        output.code, 2,
        "migrate with no source config should exit 2"
    );
    let _ = fs::remove_dir_all(&dir);
}
