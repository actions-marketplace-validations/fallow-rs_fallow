//! semantic-release plugin.
//!
//! Detects semantic-release projects and marks config files as always used.
//! Parses config to extract plugin references as dependencies.

use std::path::Path;

use super::config_parser;
use super::{Plugin, PluginResult};

pub struct SemanticReleasePlugin;

const ENABLERS: &[&str] = &["semantic-release"];

const CONFIG_PATTERNS: &[&str] = &["release.config.{js,cjs,mjs}", ".releaserc.{js,cjs}"];

const ALWAYS_USED: &[&str] = &[
    "release.config.{js,cjs,mjs}",
    ".releaserc.{json,yaml,yml,js,cjs}",
];

const TOOLING_DEPENDENCIES: &[&str] = &[
    "semantic-release",
    "@semantic-release/commit-analyzer",
    "@semantic-release/release-notes-generator",
    "@semantic-release/changelog",
    "@semantic-release/npm",
    "@semantic-release/github",
    "@semantic-release/git",
];

impl Plugin for SemanticReleasePlugin {
    fn name(&self) -> &'static str {
        "semantic-release"
    }

    fn enablers(&self) -> &'static [&'static str] {
        ENABLERS
    }

    fn config_patterns(&self) -> &'static [&'static str] {
        CONFIG_PATTERNS
    }

    fn always_used(&self) -> &'static [&'static str] {
        ALWAYS_USED
    }

    fn tooling_dependencies(&self) -> &'static [&'static str] {
        TOOLING_DEPENDENCIES
    }

    fn resolve_config(&self, config_path: &Path, source: &str, _root: &Path) -> PluginResult {
        let mut result = PluginResult::default();

        let imports = config_parser::extract_imports(source, config_path);
        for imp in &imports {
            let dep = crate::resolve::extract_package_name(imp);
            result.referenced_dependencies.push(dep);
        }

        // plugins → referenced dependencies (shallow to avoid options objects)
        let plugins = config_parser::extract_config_shallow_strings(source, config_path, "plugins");
        for plugin in &plugins {
            let dep = crate::resolve::extract_package_name(plugin);
            result.referenced_dependencies.push(dep);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_config_plugins_shallow_strings() {
        let source = r#"
            module.exports = {
                plugins: [
                    "@semantic-release/commit-analyzer",
                    "@semantic-release/release-notes-generator",
                    "@semantic-release/npm",
                    "@semantic-release/github"
                ]
            };
        "#;
        let plugin = SemanticReleasePlugin;
        let result = plugin.resolve_config(
            Path::new("release.config.js"),
            source,
            Path::new("/project"),
        );
        let deps = &result.referenced_dependencies;
        assert!(deps.contains(&"@semantic-release/commit-analyzer".to_string()));
        assert!(deps.contains(&"@semantic-release/release-notes-generator".to_string()));
        assert!(deps.contains(&"@semantic-release/npm".to_string()));
        assert!(deps.contains(&"@semantic-release/github".to_string()));
    }

    #[test]
    fn resolve_config_plugins_with_options_skipped() {
        // Shallow extraction should pick up string elements but skip array/object elements
        let source = r#"
            module.exports = {
                plugins: [
                    "@semantic-release/commit-analyzer",
                    ["@semantic-release/release-notes-generator", { preset: "angular" }],
                    "@semantic-release/npm"
                ]
            };
        "#;
        let plugin = SemanticReleasePlugin;
        let result = plugin.resolve_config(
            Path::new("release.config.js"),
            source,
            Path::new("/project"),
        );
        let deps = &result.referenced_dependencies;
        assert!(deps.contains(&"@semantic-release/commit-analyzer".to_string()));
        assert!(deps.contains(&"@semantic-release/npm".to_string()));
    }

    #[test]
    fn resolve_config_imports() {
        let source = r#"
            import { createConfig } from 'semantic-release';
            module.exports = {
                plugins: ["@semantic-release/npm"]
            };
        "#;
        let plugin = SemanticReleasePlugin;
        let result = plugin.resolve_config(
            Path::new("release.config.mjs"),
            source,
            Path::new("/project"),
        );
        let deps = &result.referenced_dependencies;
        assert!(deps.contains(&"semantic-release".to_string()));
        assert!(deps.contains(&"@semantic-release/npm".to_string()));
    }

    #[test]
    fn resolve_config_empty() {
        let source = r"module.exports = {};";
        let plugin = SemanticReleasePlugin;
        let result = plugin.resolve_config(
            Path::new("release.config.js"),
            source,
            Path::new("/project"),
        );
        assert!(result.referenced_dependencies.is_empty());
    }

    #[test]
    fn resolve_config_no_plugins() {
        let source = r#"
            module.exports = {
                branches: ["main", "next"]
            };
        "#;
        let plugin = SemanticReleasePlugin;
        let result = plugin.resolve_config(
            Path::new("release.config.js"),
            source,
            Path::new("/project"),
        );
        assert!(result.referenced_dependencies.is_empty());
    }

    #[test]
    fn resolve_config_scoped_plugin_name() {
        let source = r#"
            module.exports = {
                plugins: ["@semantic-release/git"]
            };
        "#;
        let plugin = SemanticReleasePlugin;
        let result = plugin.resolve_config(
            Path::new("release.config.cjs"),
            source,
            Path::new("/project"),
        );
        assert!(
            result
                .referenced_dependencies
                .contains(&"@semantic-release/git".to_string())
        );
    }
}
