//! tsdown TypeScript library bundler plugin.
//!
//! Detects tsdown projects and marks config files as always used.
//! Parses tsdown config to extract referenced dependencies.

use std::path::Path;

use super::config_parser;
use super::{Plugin, PluginResult};

pub struct TsdownPlugin;

const ENABLERS: &[&str] = &["tsdown"];

const CONFIG_PATTERNS: &[&str] = &["tsdown.config.{ts,js,cjs,mjs}"];

const ALWAYS_USED: &[&str] = &["tsdown.config.{ts,js,cjs,mjs}"];

const TOOLING_DEPENDENCIES: &[&str] = &["tsdown"];

impl Plugin for TsdownPlugin {
    fn name(&self) -> &'static str {
        "tsdown"
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

        // entry → source entry points for the library
        let entries = config_parser::extract_config_string_array(source, config_path, &["entry"]);
        result.entry_patterns.extend(entries);

        result
    }
}
