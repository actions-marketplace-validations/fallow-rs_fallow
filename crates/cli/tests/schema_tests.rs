#[path = "common/mod.rs"]
mod common;

use common::{parse_json, run_fallow_raw};

// ---------------------------------------------------------------------------
// schema command
// ---------------------------------------------------------------------------

#[test]
fn schema_outputs_valid_json() {
    let output = run_fallow_raw(&["schema"]);
    assert_eq!(output.code, 0, "schema should exit 0");
    let json = parse_json(&output);
    assert!(json.is_object(), "schema output should be a JSON object");
}

#[test]
fn schema_has_name_and_version() {
    let output = run_fallow_raw(&["schema"]);
    let json = parse_json(&output);
    assert_eq!(
        json["name"].as_str().unwrap(),
        "fallow",
        "schema name should be 'fallow'"
    );
    assert!(
        json.get("version").is_some(),
        "schema should have version field"
    );
}

#[test]
fn schema_has_commands_array() {
    let output = run_fallow_raw(&["schema"]);
    let json = parse_json(&output);
    let commands = json["commands"].as_array().unwrap();
    assert!(!commands.is_empty(), "schema should list commands");

    let names: Vec<&str> = commands
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"audit"), "should list audit command");
    assert!(
        names.contains(&"dead-code"),
        "should list dead-code command"
    );
    assert!(names.contains(&"health"), "should list health command");
    assert!(names.contains(&"dupes"), "should list dupes command");
}

#[test]
fn schema_has_issue_types() {
    let output = run_fallow_raw(&["schema"]);
    let json = parse_json(&output);
    let types = json["issue_types"].as_array().unwrap();
    assert!(!types.is_empty(), "schema should list issue types");
}

#[test]
fn schema_has_exit_codes() {
    let output = run_fallow_raw(&["schema"]);
    let json = parse_json(&output);
    assert!(
        json.get("exit_codes").is_some(),
        "schema should document exit codes"
    );
}

// ---------------------------------------------------------------------------
// config-schema command
// ---------------------------------------------------------------------------

#[test]
fn config_schema_outputs_valid_json() {
    let output = run_fallow_raw(&["config-schema"]);
    assert_eq!(output.code, 0, "config-schema should exit 0");
    let json = parse_json(&output);
    assert!(json.is_object(), "config-schema should be a JSON object");
}

#[test]
fn config_schema_is_json_schema() {
    let output = run_fallow_raw(&["config-schema"]);
    let json = parse_json(&output);
    assert!(
        json.get("$schema").is_some() || json.get("type").is_some(),
        "config-schema should be a JSON Schema document"
    );
}

// ---------------------------------------------------------------------------
// plugin-schema command
// ---------------------------------------------------------------------------

#[test]
fn plugin_schema_outputs_valid_json() {
    let output = run_fallow_raw(&["plugin-schema"]);
    assert_eq!(output.code, 0, "plugin-schema should exit 0");
    let json = parse_json(&output);
    assert!(json.is_object(), "plugin-schema should be a JSON object");
}

#[test]
fn plugin_schema_is_json_schema() {
    let output = run_fallow_raw(&["plugin-schema"]);
    let json = parse_json(&output);
    assert!(
        json.get("$schema").is_some() || json.get("type").is_some(),
        "plugin-schema should be a JSON Schema document"
    );
}
