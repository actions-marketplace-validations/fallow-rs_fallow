use serde_json::{Map, Value};

use super::{MigrationWarning, string_or_array};

type JsonMap = Map<String, Value>;

/// Knip rule names mapped to fallow rule names.
const KNIP_RULE_MAP: &[(&str, &str)] = &[
    ("files", "unused-files"),
    ("dependencies", "unused-dependencies"),
    ("devDependencies", "unused-dev-dependencies"),
    ("exports", "unused-exports"),
    ("types", "unused-types"),
    ("enumMembers", "unused-enum-members"),
    ("classMembers", "unused-class-members"),
    ("unlisted", "unlisted-dependencies"),
    ("unresolved", "unresolved-imports"),
    ("duplicates", "duplicate-exports"),
];

/// Knip fields that cannot be mapped and generate warnings.
const KNIP_UNMAPPABLE_FIELDS: &[(&str, &str, Option<&str>)] = &[
    ("project", "Fallow auto-discovers project files", None),
    (
        "paths",
        "Fallow reads path mappings from tsconfig.json automatically",
        None,
    ),
    (
        "ignoreFiles",
        "No separate concept in fallow",
        Some("use the `ignorePatterns` field instead"),
    ),
    (
        "ignoreBinaries",
        "Binary filtering is not configurable in fallow",
        None,
    ),
    (
        "ignoreMembers",
        "Member-level ignoring is not configurable in fallow",
        Some("use inline suppression comments: // fallow-ignore-next-line"),
    ),
    (
        "ignoreUnresolved",
        "Unresolved import filtering is not configurable in fallow",
        Some("use inline suppression comments: // fallow-ignore-next-line unresolved-imports"),
    ),
    ("ignoreExportsUsedInFile", "No equivalent in fallow", None),
    (
        "ignoreWorkspaces",
        "Workspace filtering is not configurable per-workspace",
        Some("use --workspace flag to scope output to a single package"),
    ),
    (
        "ignoreIssues",
        "No global issue ignoring in fallow",
        Some("use inline suppression comments: // fallow-ignore-file [issue-type]"),
    ),
    (
        "includeEntryExports",
        "Entry export inclusion is not configurable in fallow",
        None,
    ),
    (
        "tags",
        "Tag-based filtering is not supported in fallow",
        None,
    ),
    (
        "compilers",
        "Custom compilers are not supported in fallow (uses Oxc parser)",
        None,
    ),
    ("treatConfigHintsAsErrors", "No equivalent in fallow", None),
];

/// Knip issue type names that have no fallow equivalent.
const KNIP_UNMAPPABLE_ISSUE_TYPES: &[&str] = &[
    "optionalPeerDependencies",
    "binaries",
    "nsExports",
    "nsTypes",
    "catalog",
];

/// Known knip plugin config keys (framework-specific). These are auto-detected by fallow plugins.
const KNIP_PLUGIN_KEYS: &[&str] = &[
    "angular",
    "astro",
    "ava",
    "babel",
    "biome",
    "capacitor",
    "changesets",
    "commitizen",
    "commitlint",
    "cspell",
    "cucumber",
    "cypress",
    "docusaurus",
    "drizzle",
    "eleventy",
    "eslint",
    "expo",
    "gatsby",
    "github-actions",
    "graphql-codegen",
    "husky",
    "jest",
    "knex",
    "lefthook",
    "lint-staged",
    "markdownlint",
    "mocha",
    "moonrepo",
    "msw",
    "nest",
    "next",
    "node-test-runner",
    "npm-package-json-lint",
    "nuxt",
    "nx",
    "nyc",
    "oclif",
    "playwright",
    "postcss",
    "prettier",
    "prisma",
    "react-cosmos",
    "react-router",
    "release-it",
    "remark",
    "remix",
    "rollup",
    "rspack",
    "semantic-release",
    "sentry",
    "simple-git-hooks",
    "size-limit",
    "storybook",
    "stryker",
    "stylelint",
    "svelte",
    "syncpack",
    "tailwind",
    "tsup",
    "tsx",
    "typedoc",
    "typescript",
    "unbuild",
    "unocss",
    "vercel-og",
    "vite",
    "vitest",
    "vue",
    "webpack",
    "wireit",
    "wrangler",
    "xo",
    "yorkie",
];

/// Migrate a string-or-array field from knip to a fallow config field.
fn migrate_simple_field(obj: &JsonMap, src_key: &str, dst_key: &str, config: &mut JsonMap) {
    if let Some(val) = obj.get(src_key) {
        let entries = string_or_array(val);
        if !entries.is_empty() {
            config.insert(
                dst_key.to_string(),
                Value::Array(entries.into_iter().map(Value::String).collect()),
            );
        }
    }
}

/// Migrate knip `rules` to fallow `rules`, warning about unmappable rule names.
fn migrate_rules(rules_val: &Value, config: &mut JsonMap, warnings: &mut Vec<MigrationWarning>) {
    let Some(rules_obj) = rules_val.as_object() else {
        return;
    };

    let mut fallow_rules = Map::new();
    for (knip_name, fallow_name) in KNIP_RULE_MAP {
        if let Some(severity_val) = rules_obj.get(*knip_name)
            && let Some(severity_str) = severity_val.as_str()
        {
            fallow_rules.insert(
                (*fallow_name).to_string(),
                Value::String(severity_str.to_string()),
            );
        }
    }

    // Warn about unmappable rule names
    for (key, _) in rules_obj {
        let is_mapped = KNIP_RULE_MAP.iter().any(|(k, _)| k == key);
        let is_unmappable = KNIP_UNMAPPABLE_ISSUE_TYPES.contains(&key.as_str());
        if !is_mapped && is_unmappable {
            warnings.push(MigrationWarning {
                source: "knip",
                field: format!("rules.{key}"),
                message: format!("issue type `{key}` has no fallow equivalent"),
                suggestion: None,
            });
        }
    }

    if !fallow_rules.is_empty() {
        config.insert("rules".to_string(), Value::Object(fallow_rules));
    }
}

/// Migrate knip `exclude` — set excluded issue types to `"off"` in fallow rules.
fn migrate_exclude(
    excluded: &[String],
    config: &mut JsonMap,
    warnings: &mut Vec<MigrationWarning>,
) {
    let rules = config
        .entry("rules".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(rules_obj) = rules.as_object_mut() else {
        return;
    };

    for knip_name in excluded {
        if let Some((_, fallow_name)) = KNIP_RULE_MAP.iter().find(|(k, _)| k == knip_name) {
            rules_obj.insert((*fallow_name).to_string(), Value::String("off".to_string()));
        } else if KNIP_UNMAPPABLE_ISSUE_TYPES.contains(&knip_name.as_str()) {
            warnings.push(MigrationWarning {
                source: "knip",
                field: format!("exclude.{knip_name}"),
                message: format!("issue type `{knip_name}` has no fallow equivalent"),
                suggestion: None,
            });
        }
    }
}

/// Migrate knip `include` — set non-included issue types to `"off"` in fallow rules.
fn migrate_include(
    included: &[String],
    config: &mut JsonMap,
    warnings: &mut Vec<MigrationWarning>,
) {
    let rules = config
        .entry("rules".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(rules_obj) = rules.as_object_mut() else {
        return;
    };

    for (knip_name, fallow_name) in KNIP_RULE_MAP {
        if !included.iter().any(|i| i == knip_name) {
            // Not included -- set to off (unless already set by rules)
            rules_obj
                .entry((*fallow_name).to_string())
                .or_insert_with(|| Value::String("off".to_string()));
        }
    }
    // Warn about unmappable included types
    for name in included {
        let is_mapped = KNIP_RULE_MAP.iter().any(|(k, _)| k == name);
        if !is_mapped && KNIP_UNMAPPABLE_ISSUE_TYPES.contains(&name.as_str()) {
            warnings.push(MigrationWarning {
                source: "knip",
                field: format!("include.{name}"),
                message: format!("issue type `{name}` has no fallow equivalent"),
                suggestion: None,
            });
        }
    }
}

/// Migrate knip `ignoreDependencies` — filter out regex patterns with warnings.
fn migrate_ignore_deps(
    ignore_deps_val: &Value,
    config: &mut JsonMap,
    warnings: &mut Vec<MigrationWarning>,
) {
    let deps = string_or_array(ignore_deps_val);
    let non_regex: Vec<String> = deps
        .into_iter()
        .filter(|d| {
            // Skip values that look like regex patterns
            if d.starts_with('/') && d.ends_with('/') {
                warnings.push(MigrationWarning {
                    source: "knip",
                    field: "ignoreDependencies".to_string(),
                    message: format!("regex pattern `{d}` skipped (fallow uses exact strings)"),
                    suggestion: Some("add each dependency name explicitly".to_string()),
                });
                false
            } else {
                true
            }
        })
        .collect();
    if !non_regex.is_empty() {
        config.insert(
            "ignoreDependencies".to_string(),
            Value::Array(non_regex.into_iter().map(Value::String).collect()),
        );
    }
}

/// Warn about knip fields that have no fallow equivalent.
fn warn_unmappable_fields(obj: &JsonMap, warnings: &mut Vec<MigrationWarning>) {
    for (field, message, suggestion) in KNIP_UNMAPPABLE_FIELDS {
        if obj.contains_key(*field) {
            warnings.push(MigrationWarning {
                source: "knip",
                field: (*field).to_string(),
                message: (*message).to_string(),
                suggestion: suggestion.map(|s| s.to_string()),
            });
        }
    }
}

/// Warn about knip plugin-specific config keys that are auto-detected in fallow.
fn warn_plugin_keys(obj: &JsonMap, warnings: &mut Vec<MigrationWarning>) {
    for key in obj.keys() {
        if KNIP_PLUGIN_KEYS.contains(&key.as_str()) {
            warnings.push(MigrationWarning {
                source: "knip",
                field: key.clone(),
                message: format!(
                    "plugin config `{key}` is auto-detected by fallow's built-in plugins"
                ),
                suggestion: Some(
                    "remove this section; fallow detects framework config automatically"
                        .to_string(),
                ),
            });
        }
    }
}

pub(super) fn migrate_knip(
    knip: &Value,
    config: &mut JsonMap,
    warnings: &mut Vec<MigrationWarning>,
) {
    let Some(obj) = knip.as_object() else {
        warnings.push(MigrationWarning {
            source: "knip",
            field: "(root)".to_string(),
            message: "expected an object, got something else".to_string(),
            suggestion: None,
        });
        return;
    };

    // entry -> entry
    migrate_simple_field(obj, "entry", "entry", config);

    // ignore -> ignorePatterns
    migrate_simple_field(obj, "ignore", "ignorePatterns", config);

    // ignoreDependencies -> ignoreDependencies (skip regex values)
    if let Some(ignore_deps_val) = obj.get("ignoreDependencies") {
        migrate_ignore_deps(ignore_deps_val, config, warnings);
    }

    // rules -> rules mapping
    if let Some(rules_val) = obj.get("rules") {
        migrate_rules(rules_val, config, warnings);
    }

    // exclude -> set those issue types to "off" in rules
    if let Some(exclude_val) = obj.get("exclude") {
        let excluded = string_or_array(exclude_val);
        if !excluded.is_empty() {
            migrate_exclude(&excluded, config, warnings);
        }
    }

    // include -> set non-included issue types to "off" in rules
    if let Some(include_val) = obj.get("include") {
        let included = string_or_array(include_val);
        if !included.is_empty() {
            migrate_include(&included, config, warnings);
        }
    }

    // Warn about unmappable fields
    warn_unmappable_fields(obj, warnings);

    // Warn about plugin-specific config keys
    warn_plugin_keys(obj, warnings);

    // Warn about workspaces with per-workspace plugin overrides
    if let Some(workspaces_val) = obj.get("workspaces")
        && workspaces_val.is_object()
    {
        warnings.push(MigrationWarning {
            source: "knip",
            field: "workspaces".to_string(),
            message: "per-workspace plugin overrides have limited support in fallow".to_string(),
            suggestion: Some(
                "fallow auto-discovers workspace packages; use --workspace flag to scope output"
                    .to_string(),
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_config() -> serde_json::Map<String, serde_json::Value> {
        serde_json::Map::new()
    }

    #[test]
    fn migrate_minimal_knip_json() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"entry": ["src/index.ts"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(
            config.get("entry").unwrap(),
            &serde_json::json!(["src/index.ts"])
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn migrate_knip_with_rules() {
        let knip: serde_json::Value = serde_json::from_str(
            r#"{"rules": {"files": "warn", "exports": "off", "dependencies": "error"}}"#,
        )
        .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        let rules = config.get("rules").unwrap().as_object().unwrap();
        assert_eq!(rules.get("unused-files").unwrap(), "warn");
        assert_eq!(rules.get("unused-exports").unwrap(), "off");
        assert_eq!(rules.get("unused-dependencies").unwrap(), "error");
    }

    #[test]
    fn migrate_knip_with_exclude() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"exclude": ["files", "types"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        let rules = config.get("rules").unwrap().as_object().unwrap();
        assert_eq!(rules.get("unused-files").unwrap(), "off");
        assert_eq!(rules.get("unused-types").unwrap(), "off");
    }

    #[test]
    fn migrate_knip_with_include() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"include": ["files", "exports"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        let rules = config.get("rules").unwrap().as_object().unwrap();
        // Included types are not inserted into rules (they keep their default)
        assert!(
            !rules.contains_key("unused-files"),
            "included type 'unused-files' should not be in rules"
        );
        assert!(
            !rules.contains_key("unused-exports"),
            "included type 'unused-exports' should not be in rules"
        );
        // Non-included types should be "off"
        assert_eq!(rules.get("unused-dependencies").unwrap(), "off");
        assert_eq!(rules.get("unused-dev-dependencies").unwrap(), "off");
        assert_eq!(rules.get("unused-types").unwrap(), "off");
        assert_eq!(rules.get("unused-enum-members").unwrap(), "off");
        assert_eq!(rules.get("unused-class-members").unwrap(), "off");
        assert_eq!(rules.get("unlisted-dependencies").unwrap(), "off");
        assert_eq!(rules.get("unresolved-imports").unwrap(), "off");
        assert_eq!(rules.get("duplicate-exports").unwrap(), "off");
    }

    #[test]
    fn migrate_knip_with_ignore_patterns() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"ignore": ["src/generated/**", "**/*.test.ts"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(
            config.get("ignorePatterns").unwrap(),
            &serde_json::json!(["src/generated/**", "**/*.test.ts"])
        );
    }

    #[test]
    fn migrate_knip_with_ignore_dependencies() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"ignoreDependencies": ["@org/lib", "lodash"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(
            config.get("ignoreDependencies").unwrap(),
            &serde_json::json!(["@org/lib", "lodash"])
        );
    }

    #[test]
    fn migrate_knip_regex_ignore_deps_skipped() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"ignoreDependencies": ["/^@org/", "lodash"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(
            config.get("ignoreDependencies").unwrap(),
            &serde_json::json!(["lodash"])
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].field == "ignoreDependencies");
    }

    #[test]
    fn migrate_knip_unmappable_fields_generate_warnings() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"project": ["src/**"], "paths": {"@/*": ["src/*"]}}"#)
                .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(warnings.len(), 2);
        let fields: Vec<&str> = warnings.iter().map(|w| w.field.as_str()).collect();
        assert!(fields.contains(&"project"));
        assert!(fields.contains(&"paths"));
    }

    #[test]
    fn migrate_knip_plugin_keys_generate_warnings() {
        let knip: serde_json::Value = serde_json::from_str(
            r#"{"entry": ["src/index.ts"], "eslint": {"entry": ["eslint.config.js"]}}"#,
        )
        .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].field, "eslint");
        assert!(warnings[0].message.contains("auto-detected"));
    }

    #[test]
    fn migrate_knip_entry_string() {
        let knip: serde_json::Value = serde_json::from_str(r#"{"entry": "src/index.ts"}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(
            config.get("entry").unwrap(),
            &serde_json::json!(["src/index.ts"])
        );
    }

    #[test]
    fn migrate_knip_exclude_unmappable_warns() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"exclude": ["optionalPeerDependencies"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].field.contains("optionalPeerDependencies"));
    }

    #[test]
    fn migrate_knip_rules_unmappable_warns() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"rules": {"binaries": "warn", "files": "error"}}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        let rules = config.get("rules").unwrap().as_object().unwrap();
        assert_eq!(rules.get("unused-files").unwrap(), "error");
        assert!(!rules.contains_key("binaries"));

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].field.contains("binaries"));
    }

    // -- Non-object root produces warning ------------------------------------

    #[test]
    fn migrate_knip_non_object_root_warns() {
        let knip: serde_json::Value = serde_json::json!("not an object");
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].field, "(root)");
        assert!(warnings[0].message.contains("expected an object"));
        // Config should remain empty
        assert!(config.is_empty());
    }

    // -- Workspaces warning --------------------------------------------------

    #[test]
    fn migrate_knip_workspaces_object_warns() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"workspaces": {"packages/*": {"entry": ["src/index.ts"]}}}"#)
                .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].field, "workspaces");
        assert!(
            warnings[0]
                .message
                .contains("per-workspace plugin overrides")
        );
        assert!(warnings[0].suggestion.is_some());
    }

    #[test]
    fn migrate_knip_workspaces_non_object_no_warning() {
        // workspaces as an array should NOT trigger the warning
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"workspaces": ["packages/*"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        // No workspace warning since it's not an object
        assert!(!warnings.iter().any(|w| w.field == "workspaces"));
    }

    // -- All regex deps filtered produces no ignoreDependencies key ----------

    #[test]
    fn migrate_knip_all_regex_ignore_deps_no_output() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"ignoreDependencies": ["/^@org/", "/^lodash/"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        // All deps were regex, so ignoreDependencies should NOT be in config
        assert!(!config.contains_key("ignoreDependencies"));
        assert_eq!(warnings.len(), 2);
    }

    // -- ignoreDependencies as a single string -------------------------------

    #[test]
    fn migrate_knip_ignore_deps_single_string() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"ignoreDependencies": "lodash"}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(
            config.get("ignoreDependencies").unwrap(),
            &serde_json::json!(["lodash"])
        );
    }

    // -- Rules with non-string severity values are skipped -------------------

    #[test]
    fn migrate_knip_rules_non_string_severity_ignored() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"rules": {"files": 123, "exports": "warn"}}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        let rules = config.get("rules").unwrap().as_object().unwrap();
        // "files" had numeric severity -> skipped
        assert!(!rules.contains_key("unused-files"));
        // "exports" is valid
        assert_eq!(rules.get("unused-exports").unwrap(), "warn");
    }

    // -- Rules field that is not an object -----------------------------------

    #[test]
    fn migrate_knip_rules_non_object_ignored() {
        let knip: serde_json::Value = serde_json::from_str(r#"{"rules": "invalid"}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        // rules should not be added to config
        assert!(!config.contains_key("rules"));
        assert!(warnings.is_empty());
    }

    // -- include with unmappable types warns ---------------------------------

    #[test]
    fn migrate_knip_include_unmappable_warns() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"include": ["files", "binaries"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        // "binaries" is unmappable
        let include_warnings: Vec<_> = warnings
            .iter()
            .filter(|w| w.field.starts_with("include."))
            .collect();
        assert_eq!(include_warnings.len(), 1);
        assert!(include_warnings[0].field.contains("binaries"));
    }

    // -- include interacts with rules: rules take precedence -----------------

    #[test]
    fn migrate_knip_rules_then_include_rules_take_precedence() {
        // If both rules and include are set, rules should set values first,
        // then include fills in "off" for non-included types using or_insert
        let knip: serde_json::Value = serde_json::from_str(
            r#"{"rules": {"dependencies": "warn"}, "include": ["files", "dependencies"]}"#,
        )
        .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        let rules = config.get("rules").unwrap().as_object().unwrap();
        // "dependencies" was set to "warn" by rules, include should NOT override it
        assert_eq!(rules.get("unused-dependencies").unwrap(), "warn");
        // "exports" was not included -> "off"
        assert_eq!(rules.get("unused-exports").unwrap(), "off");
        // "files" was included and not in rules -> should not be present at all
        assert!(
            !rules.contains_key("unused-files"),
            "included type 'unused-files' should not be in rules"
        );
    }

    // -- Multiple unmappable fields with suggestions -------------------------

    #[test]
    fn migrate_knip_multiple_unmappable_fields_with_suggestions() {
        let knip: serde_json::Value = serde_json::from_str(
            r#"{"ignoreFiles": ["x.ts"], "ignoreMembers": ["id"], "ignoreUnresolved": ["y"]}"#,
        )
        .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(warnings.len(), 3);
        // All three should have suggestions
        for w in &warnings {
            assert!(
                w.suggestion.is_some(),
                "warning for `{}` should have a suggestion",
                w.field
            );
        }
    }

    // -- Multiple plugin keys warn separately --------------------------------

    #[test]
    fn migrate_knip_multiple_plugin_keys_warn() {
        let knip: serde_json::Value = serde_json::from_str(
            r#"{"eslint": {"entry": ["a.js"]}, "jest": {"entry": ["b.js"]}, "vitest": true}"#,
        )
        .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(
            warnings
                .iter()
                .filter(|w| w.message.contains("auto-detected"))
                .count(),
            3
        );
    }

    // -- All rule mappings are covered ---------------------------------------

    #[test]
    fn migrate_knip_all_rule_mappings() {
        let knip: serde_json::Value = serde_json::from_str(
            r#"{"rules": {
                "files": "error",
                "dependencies": "warn",
                "devDependencies": "off",
                "exports": "error",
                "types": "warn",
                "enumMembers": "error",
                "classMembers": "warn",
                "unlisted": "error",
                "unresolved": "warn",
                "duplicates": "off"
            }}"#,
        )
        .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        let rules = config.get("rules").unwrap().as_object().unwrap();
        assert_eq!(rules.get("unused-files").unwrap(), "error");
        assert_eq!(rules.get("unused-dependencies").unwrap(), "warn");
        assert_eq!(rules.get("unused-dev-dependencies").unwrap(), "off");
        assert_eq!(rules.get("unused-exports").unwrap(), "error");
        assert_eq!(rules.get("unused-types").unwrap(), "warn");
        assert_eq!(rules.get("unused-enum-members").unwrap(), "error");
        assert_eq!(rules.get("unused-class-members").unwrap(), "warn");
        assert_eq!(rules.get("unlisted-dependencies").unwrap(), "error");
        assert_eq!(rules.get("unresolved-imports").unwrap(), "warn");
        assert_eq!(rules.get("duplicate-exports").unwrap(), "off");
        assert!(warnings.is_empty());
    }

    // -- Exclude all mappable types -----------------------------------------

    #[test]
    fn migrate_knip_exclude_all_mappable_types() {
        let knip: serde_json::Value = serde_json::from_str(
            r#"{"exclude": ["files", "dependencies", "devDependencies", "exports",
                "types", "enumMembers", "classMembers", "unlisted", "unresolved", "duplicates"]}"#,
        )
        .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        let rules = config.get("rules").unwrap().as_object().unwrap();
        // All should be "off"
        for (_, fallow_name) in KNIP_RULE_MAP {
            assert_eq!(
                rules.get(*fallow_name).unwrap(),
                "off",
                "{fallow_name} should be off"
            );
        }
        assert!(warnings.is_empty());
    }

    // -- Empty entry/ignore produce no config keys ---------------------------

    #[test]
    fn migrate_knip_empty_entry_array() {
        let knip: serde_json::Value = serde_json::from_str(r#"{"entry": []}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        // Empty array should not produce an "entry" key
        assert!(!config.contains_key("entry"));
    }

    #[test]
    fn migrate_knip_empty_ignore_array() {
        let knip: serde_json::Value = serde_json::from_str(r#"{"ignore": []}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert!(!config.contains_key("ignorePatterns"));
    }

    // -- Unmappable fields that DON'T have suggestions ----------------------

    #[test]
    fn migrate_knip_unmappable_without_suggestion() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"ignoreBinaries": ["tsc"], "ignoreExportsUsedInFile": true}"#)
                .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(warnings.len(), 2);
        // Both should have no suggestion
        assert_eq!(
            warnings.iter().filter(|w| w.suggestion.is_none()).count(),
            2
        );
    }

    // -- Rules with unknown (non-knip) keys are silently ignored ------------

    #[test]
    fn migrate_knip_rules_unknown_key_not_in_unmappable_silently_ignored() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"rules": {"completelyUnknownRule": "warn"}}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        // Not in KNIP_RULE_MAP and not in KNIP_UNMAPPABLE_ISSUE_TYPES -> silently ignored
        assert!(warnings.is_empty());
        // rules map might be empty so no "rules" key
        assert!(!config.contains_key("rules"));
    }

    // -- Combined complex migration -----------------------------------------

    #[test]
    fn migrate_knip_complex_full_config() {
        let knip: serde_json::Value = serde_json::from_str(
            r#"{
                "entry": ["src/index.ts", "src/worker.ts"],
                "ignore": ["**/*.generated.*"],
                "ignoreDependencies": ["/^@internal/", "lodash", "react"],
                "rules": {"files": "warn", "exports": "error"},
                "exclude": ["types"],
                "project": ["src/**"],
                "eslint": {"entry": ["eslint.config.js"]},
                "workspaces": {"packages/*": {"entry": ["index.ts"]}}
            }"#,
        )
        .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        // Verify config fields
        assert_eq!(
            config.get("entry").unwrap(),
            &serde_json::json!(["src/index.ts", "src/worker.ts"])
        );
        assert_eq!(
            config.get("ignorePatterns").unwrap(),
            &serde_json::json!(["**/*.generated.*"])
        );
        assert_eq!(
            config.get("ignoreDependencies").unwrap(),
            &serde_json::json!(["lodash", "react"])
        );

        let rules = config.get("rules").unwrap().as_object().unwrap();
        assert_eq!(rules.get("unused-files").unwrap(), "warn");
        assert_eq!(rules.get("unused-exports").unwrap(), "error");
        assert_eq!(rules.get("unused-types").unwrap(), "off");

        // Verify warnings: regex dep + project + eslint plugin + workspaces
        let warning_fields: Vec<&str> = warnings.iter().map(|w| w.field.as_str()).collect();
        assert!(warning_fields.contains(&"ignoreDependencies"));
        assert!(warning_fields.contains(&"project"));
        assert!(warning_fields.contains(&"eslint"));
        assert!(warning_fields.contains(&"workspaces"));
    }
}
