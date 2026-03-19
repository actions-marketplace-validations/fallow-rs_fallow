//! Nuxt framework plugin.
//!
//! Detects Nuxt projects and marks pages, layouts, middleware, server API,
//! plugins, composables, and utils as entry points. Recognizes conventional
//! server API and middleware exports. Parses nuxt.config.ts to extract modules,
//! CSS files, plugins, and other configuration.

use std::path::Path;

use super::config_parser;
use super::{Plugin, PluginResult};

const ENABLERS: &[&str] = &["nuxt"];

const ENTRY_PATTERNS: &[&str] = &[
    // Standard Nuxt directories
    "pages/**/*.{vue,ts,tsx,js,jsx}",
    "layouts/**/*.{vue,ts,tsx,js,jsx}",
    "middleware/**/*.{ts,js}",
    "server/api/**/*.{ts,js}",
    "server/routes/**/*.{ts,js}",
    "server/middleware/**/*.{ts,js}",
    "plugins/**/*.{ts,js}",
    "composables/**/*.{ts,js}",
    "utils/**/*.{ts,js}",
    "components/**/*.{vue,ts,tsx,js,jsx}",
    // Nuxt 3 app/ directory structure
    "app/pages/**/*.{vue,ts,tsx,js,jsx}",
    "app/layouts/**/*.{vue,ts,tsx,js,jsx}",
    "app/middleware/**/*.{ts,js}",
    "app/plugins/**/*.{ts,js}",
    "app/composables/**/*.{ts,js}",
    "app/utils/**/*.{ts,js}",
    "app/components/**/*.{vue,ts,tsx,js,jsx}",
];

const CONFIG_PATTERNS: &[&str] = &["nuxt.config.{ts,js}"];

const ALWAYS_USED: &[&str] = &[
    "nuxt.config.{ts,js}",
    "app.vue",
    "app.config.{ts,js}",
    "error.vue",
    // Nuxt 3 app/ directory structure
    "app/app.vue",
    "app/error.vue",
];

/// Implicit dependencies that Nuxt provides — these should not be flagged as unlisted.
const TOOLING_DEPENDENCIES: &[&str] = &[
    "nuxt",
    "@nuxt/devtools",
    "@nuxt/test-utils",
    // Implicit Nuxt runtime dependencies
    "vue",
    "vue-router",
    "ofetch",
    "h3",
    "@unhead/vue",
    "nitropack",
    "defu",
    "hookable",
    "ufo",
];

const USED_EXPORTS_SERVER_API: &[&str] = &["default", "defineEventHandler"];
const USED_EXPORTS_MIDDLEWARE: &[&str] = &["default"];

/// Virtual module prefixes provided by Nuxt at build time.
const VIRTUAL_MODULE_PREFIXES: &[&str] = &["#"];

pub struct NuxtPlugin;

impl Plugin for NuxtPlugin {
    fn name(&self) -> &'static str {
        "nuxt"
    }

    fn enablers(&self) -> &'static [&'static str] {
        ENABLERS
    }

    fn entry_patterns(&self) -> &'static [&'static str] {
        ENTRY_PATTERNS
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

    fn virtual_module_prefixes(&self) -> &'static [&'static str] {
        VIRTUAL_MODULE_PREFIXES
    }

    fn used_exports(&self) -> Vec<(&'static str, &'static [&'static str])> {
        vec![
            ("server/api/**/*.{ts,js}", USED_EXPORTS_SERVER_API),
            ("middleware/**/*.{ts,js}", USED_EXPORTS_MIDDLEWARE),
        ]
    }

    fn resolve_config(&self, config_path: &Path, source: &str, _root: &Path) -> PluginResult {
        let mut result = PluginResult::default();

        // Extract import sources as referenced dependencies
        let imports = config_parser::extract_imports(source, config_path);
        for imp in &imports {
            let dep = crate::resolve::extract_package_name(imp);
            result.referenced_dependencies.push(dep);
        }

        // modules: [...] → referenced dependencies (Nuxt modules are npm packages)
        let modules = config_parser::extract_config_string_array(source, config_path, &["modules"]);
        for module in &modules {
            let dep = crate::resolve::extract_package_name(module);
            result.referenced_dependencies.push(dep);
        }

        // css: [...] → always-used files
        let css = config_parser::extract_config_string_array(source, config_path, &["css"]);
        result.always_used_files.extend(css);

        // postcss.plugins → referenced dependencies (object keys)
        let postcss_plugins =
            config_parser::extract_config_object_keys(source, config_path, &["postcss", "plugins"]);
        for plugin in &postcss_plugins {
            result
                .referenced_dependencies
                .push(crate::resolve::extract_package_name(plugin));
        }

        // plugins: [...] → entry patterns
        let plugins = config_parser::extract_config_string_array(source, config_path, &["plugins"]);
        result.entry_patterns.extend(plugins);

        // extends: [...] → referenced dependencies
        let extends = config_parser::extract_config_string_array(source, config_path, &["extends"]);
        for ext in &extends {
            result
                .referenced_dependencies
                .push(crate::resolve::extract_package_name(ext));
        }

        result
    }
}
