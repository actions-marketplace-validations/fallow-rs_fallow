use std::path::PathBuf;

use fallow_config::{DetectConfig, FallowConfig, OutputFormat};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn create_config(root: PathBuf) -> fallow_config::ResolvedConfig {
    FallowConfig {
        root: None,
        entry: vec![],
        ignore: vec![],
        detect: DetectConfig::default(),
        frameworks: None,
        framework: vec![],
        workspaces: None,
        ignore_dependencies: vec![],
        ignore_exports: vec![],
        output: OutputFormat::Human,
    }
    .resolve(root, 4, true)
}

#[test]
fn basic_project_detects_unused_files() {
    let root = fixture_path("basic-project");
    let config = create_config(root.clone());
    let results = fallow_core::analyze(&config);

    // orphan.ts should be detected as unused
    let unused_file_names: Vec<String> = results
        .unused_files
        .iter()
        .map(|f| {
            f.path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect();

    assert!(
        unused_file_names.contains(&"orphan.ts".to_string()),
        "orphan.ts should be detected as unused file, found: {unused_file_names:?}"
    );
}

#[test]
fn basic_project_detects_unused_exports() {
    let root = fixture_path("basic-project");
    let config = create_config(root.clone());
    let results = fallow_core::analyze(&config);

    let unused_export_names: Vec<&str> = results
        .unused_exports
        .iter()
        .map(|e| e.export_name.as_str())
        .collect();

    assert!(
        unused_export_names.contains(&"unusedFunction"),
        "unusedFunction should be detected as unused export, found: {unused_export_names:?}"
    );
    assert!(
        unused_export_names.contains(&"anotherUnused"),
        "anotherUnused should be detected as unused export, found: {unused_export_names:?}"
    );
    // usedFunction should NOT be in unused
    assert!(
        !unused_export_names.contains(&"usedFunction"),
        "usedFunction should NOT be detected as unused"
    );
}

#[test]
fn basic_project_detects_unused_types() {
    let root = fixture_path("basic-project");
    let config = create_config(root.clone());
    let results = fallow_core::analyze(&config);

    let unused_type_names: Vec<&str> = results
        .unused_types
        .iter()
        .map(|e| e.export_name.as_str())
        .collect();

    assert!(
        unused_type_names.contains(&"UnusedType"),
        "UnusedType should be detected as unused type, found: {unused_type_names:?}"
    );
    assert!(
        unused_type_names.contains(&"UnusedInterface"),
        "UnusedInterface should be detected as unused type, found: {unused_type_names:?}"
    );
    // UsedType should NOT be in unused
    assert!(
        !unused_type_names.contains(&"UsedType"),
        "UsedType should NOT be detected as unused"
    );
}

#[test]
fn basic_project_detects_unused_dependencies() {
    let root = fixture_path("basic-project");
    let config = create_config(root.clone());
    let results = fallow_core::analyze(&config);

    let unused_dep_names: Vec<&str> = results
        .unused_dependencies
        .iter()
        .map(|d| d.package_name.as_str())
        .collect();

    assert!(
        unused_dep_names.contains(&"unused-dep"),
        "unused-dep should be detected as unused dependency, found: {unused_dep_names:?}"
    );
}

#[test]
fn barrel_exports_resolves_through_barrel() {
    let root = fixture_path("barrel-exports");
    let config = create_config(root.clone());
    let results = fallow_core::analyze(&config);

    let unused_export_names: Vec<&str> = results
        .unused_exports
        .iter()
        .map(|e| e.export_name.as_str())
        .collect();

    // fooUnused should be detected as unused (it's not re-exported from barrel)
    assert!(
        unused_export_names.contains(&"fooUnused"),
        "fooUnused should be unused, found: {unused_export_names:?}"
    );
}

#[test]
fn analysis_returns_correct_total_count() {
    let root = fixture_path("basic-project");
    let config = create_config(root.clone());
    let results = fallow_core::analyze(&config);

    assert!(
        results.has_issues(),
        "basic-project should have issues"
    );
    assert!(
        results.total_issues() > 0,
        "total_issues should be > 0"
    );
}

#[test]
fn dynamic_import_is_parsed() {
    use fallow_core::extract::parse_from_content;
    use fallow_core::discover::FileId;
    use std::path::Path;

    let content = r#"const mod = import('./lazy-module');"#;
    let info = parse_from_content(FileId(0), Path::new("test.ts"), content);

    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./lazy-module");
}

#[test]
fn cjs_interop_detects_require() {
    use fallow_core::extract::parse_from_content;
    use fallow_core::discover::FileId;
    use std::path::Path;

    let content = r#"const fs = require('fs'); const path = require('path');"#;
    let info = parse_from_content(FileId(0), Path::new("test.js"), content);

    assert_eq!(info.require_calls.len(), 2);
    assert_eq!(info.require_calls[0].source, "fs");
    assert_eq!(info.require_calls[1].source, "path");
}

#[test]
fn type_only_imports_are_marked() {
    use fallow_core::extract::parse_from_content;
    use fallow_core::discover::FileId;
    use std::path::Path;

    let content = r#"import type { Foo } from './types'; import { Bar } from './utils';"#;
    let info = parse_from_content(FileId(0), Path::new("test.ts"), content);

    assert_eq!(info.imports.len(), 2);
    assert!(info.imports[0].is_type_only);
    assert!(!info.imports[1].is_type_only);
}

#[test]
fn enum_members_are_extracted() {
    use fallow_core::extract::parse_from_content;
    use fallow_core::discover::FileId;
    use std::path::Path;

    let content = r#"export enum Color { Red = 'red', Green = 'green', Blue = 'blue' }"#;
    let info = parse_from_content(FileId(0), Path::new("test.ts"), content);

    assert_eq!(info.exports.len(), 1);
    assert_eq!(info.exports[0].members.len(), 3);
    assert_eq!(info.exports[0].members[0].name, "Red");
    assert_eq!(info.exports[0].members[1].name, "Green");
    assert_eq!(info.exports[0].members[2].name, "Blue");
}

#[test]
fn class_members_are_extracted() {
    use fallow_core::extract::parse_from_content;
    use fallow_core::discover::FileId;
    use std::path::Path;

    let content = r#"
export class MyService {
    name: string = '';
    async getUser(id: number) { return id; }
    static create() { return new MyService(); }
}
"#;
    let info = parse_from_content(FileId(0), Path::new("test.ts"), content);

    assert_eq!(info.exports.len(), 1);
    assert!(info.exports[0].members.len() >= 3, "Should have at least 3 members");
}

#[test]
fn star_re_export_is_parsed() {
    use fallow_core::extract::parse_from_content;
    use fallow_core::discover::FileId;
    use std::path::Path;

    let content = r#"export * from './module';"#;
    let info = parse_from_content(FileId(0), Path::new("test.ts"), content);

    assert_eq!(info.re_exports.len(), 1);
    assert_eq!(info.re_exports[0].imported_name, "*");
    assert_eq!(info.re_exports[0].exported_name, "*");
    assert_eq!(info.re_exports[0].source, "./module");
}

#[test]
fn named_re_export_is_parsed() {
    use fallow_core::extract::parse_from_content;
    use fallow_core::discover::FileId;
    use std::path::Path;

    let content = r#"export { foo, bar as baz } from './module';"#;
    let info = parse_from_content(FileId(0), Path::new("test.ts"), content);

    assert_eq!(info.re_exports.len(), 2);
    assert_eq!(info.re_exports[0].imported_name, "foo");
    assert_eq!(info.re_exports[0].exported_name, "foo");
    assert_eq!(info.re_exports[1].imported_name, "bar");
    assert_eq!(info.re_exports[1].exported_name, "baz");
}

#[test]
fn circular_import_does_not_crash() {
    // Create temporary fixture with circular imports
    let temp_dir = std::env::temp_dir().join("fallow-test-circular");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(temp_dir.join("src")).unwrap();

    std::fs::write(
        temp_dir.join("package.json"),
        r#"{"name": "circular", "main": "src/a.ts"}"#,
    )
    .unwrap();

    std::fs::write(
        temp_dir.join("src/a.ts"),
        "import { b } from './b';\nexport const a = b + 1;\n",
    )
    .unwrap();

    std::fs::write(
        temp_dir.join("src/b.ts"),
        "import { a } from './a';\nexport const b = a + 1;\n",
    )
    .unwrap();

    let config = create_config(temp_dir.clone());
    // This should not crash or infinite loop
    let results = fallow_core::analyze(&config);
    assert!(results.total_issues() >= 0);

    let _ = std::fs::remove_dir_all(&temp_dir);
}
