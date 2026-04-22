//! `fallow coverage` - paid Production Coverage onboarding and inventory
//! upload.
//!
//! Today the subtree holds two commands:
//!
//! - `setup`: resumable first-run state machine (license + sidecar + recipe
//!   + auto-handoff to `fallow health --production-coverage`).
//! - `upload-inventory`: push a static function inventory to fallow cloud,
//!   unlocking the `untracked` filter on the dashboard by pairing runtime
//!   coverage data with the AST view of "every function that exists".

use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use fallow_config::PackageJson;
use fallow_license::{DEFAULT_HARD_FAIL_DAYS, LicenseStatus};

use crate::health::coverage as production_coverage;
use crate::license;

pub use upload_inventory::UploadInventoryArgs;

mod upload_inventory;

const COVERAGE_DOCS_URL: &str = "https://docs.fallow.tools/analysis/production-coverage";

/// Subcommands for `fallow coverage`.
#[derive(Debug, Clone)]
pub enum CoverageSubcommand {
    /// Resumable first-run setup flow.
    Setup(SetupArgs),
    /// Upload a static function inventory to fallow cloud.
    UploadInventory(UploadInventoryArgs),
}

/// Arguments for `fallow coverage setup`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SetupArgs {
    /// Accept all prompts automatically.
    pub yes: bool,
    /// Print instructions instead of prompting.
    pub non_interactive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameworkKind {
    NextJs,
    NestJs,
    Nuxt,
    SvelteKit,
    Astro,
    Remix,
    PlainNode,
    Other,
}

impl FrameworkKind {
    const fn label(self) -> &'static str {
        match self {
            Self::NextJs => "Next.js project",
            Self::NestJs => "NestJS project",
            Self::Nuxt => "Nuxt app",
            Self::SvelteKit => "SvelteKit app",
            Self::Astro => "Astro app",
            Self::Remix => "Remix app",
            Self::PlainNode => "Node service",
            Self::Other => "custom project",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    const fn label(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
        }
    }

    const fn install_args(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Npm => ("npm", &["install", "--save-dev", "@fallow-cli/fallow-cov"]),
            Self::Pnpm => ("pnpm", &["add", "-D", "@fallow-cli/fallow-cov"]),
            Self::Yarn => ("yarn", &["add", "-D", "@fallow-cli/fallow-cov"]),
            Self::Bun => ("bun", &["add", "-d", "@fallow-cli/fallow-cov"]),
        }
    }

    fn install_command(self) -> String {
        let (program, args) = self.install_args();
        format!("{program} {}", args.join(" "))
    }

    fn run_script(self, script: &str) -> String {
        match self {
            Self::Npm => format!("npm run {script}"),
            Self::Pnpm => format!("pnpm {script}"),
            Self::Yarn => format!("yarn {script}"),
            Self::Bun => format!("bun run {script}"),
        }
    }

    fn exec_binary(self, binary: &str, args: &[&str]) -> String {
        let suffix = if args.is_empty() {
            String::new()
        } else {
            format!(" {}", args.join(" "))
        };
        match self {
            Self::Npm => format!("npx {binary}{suffix}"),
            Self::Pnpm => format!("pnpm exec {binary}{suffix}"),
            Self::Yarn => format!("yarn {binary}{suffix}"),
            Self::Bun => format!("bunx {binary}{suffix}"),
        }
    }
}

#[derive(Debug, Clone)]
struct CoverageSetupContext {
    framework: FrameworkKind,
    package_manager: Option<PackageManager>,
    has_build_script: bool,
    has_start_script: bool,
    has_preview_script: bool,
}

impl CoverageSetupContext {
    fn script_runner(&self) -> PackageManager {
        self.package_manager.unwrap_or(PackageManager::Npm)
    }

    fn build_command(&self) -> Option<String> {
        if self.has_build_script {
            return Some(self.script_runner().run_script("build"));
        }
        match self.framework {
            FrameworkKind::NextJs => Some(self.script_runner().exec_binary("next", &["build"])),
            FrameworkKind::Nuxt => Some(self.script_runner().exec_binary("nuxi", &["build"])),
            FrameworkKind::Astro => Some(self.script_runner().exec_binary("astro", &["build"])),
            FrameworkKind::Remix => Some(self.script_runner().exec_binary("remix", &["build"])),
            FrameworkKind::SvelteKit => Some(self.script_runner().exec_binary("vite", &["build"])),
            FrameworkKind::NestJs | FrameworkKind::PlainNode | FrameworkKind::Other => None,
        }
    }

    fn run_command(&self) -> String {
        if self.has_preview_script
            && matches!(
                self.framework,
                FrameworkKind::Nuxt | FrameworkKind::SvelteKit | FrameworkKind::Astro
            )
        {
            return self.script_runner().run_script("preview");
        }
        if self.has_start_script {
            return self.script_runner().run_script("start");
        }
        match self.framework {
            FrameworkKind::NextJs => self.script_runner().exec_binary("next", &["start"]),
            FrameworkKind::Nuxt => self.script_runner().exec_binary("nuxi", &["preview"]),
            FrameworkKind::Astro => self.script_runner().exec_binary("astro", &["preview"]),
            FrameworkKind::SvelteKit => self.script_runner().exec_binary("vite", &["preview"]),
            FrameworkKind::Remix => "node ./build/index.js".to_owned(),
            FrameworkKind::NestJs => "node dist/main.js".to_owned(),
            FrameworkKind::PlainNode | FrameworkKind::Other => "node dist/server.js".to_owned(),
        }
    }
}

/// Dispatch a `fallow coverage <sub>` invocation.
pub fn run(subcommand: CoverageSubcommand, root: &Path) -> ExitCode {
    match subcommand {
        CoverageSubcommand::Setup(args) => run_setup(args, root),
        CoverageSubcommand::UploadInventory(args) => upload_inventory::run(&args, root),
    }
}

fn run_setup(args: SetupArgs, root: &Path) -> ExitCode {
    println!("fallow coverage setup");
    println!();
    println!("What \"production coverage\" means: fallow looks at which functions actually");
    println!("ran in your deployed app, so it can say \"this code is never called\" with");
    println!("proof, not just \"this code has no static references.\"");
    println!();

    let key = match license::verifying_key() {
        Ok(key) => key,
        Err(message) => {
            eprintln!("fallow coverage setup: {message}");
            return ExitCode::from(2);
        }
    };

    let license_state = fallow_license::load_and_verify(&key, DEFAULT_HARD_FAIL_DAYS);
    if let Some(exit) = handle_license_step(root, args, &license_state) {
        return exit;
    }

    let context = detect_setup_context(root);

    if let Some(exit) = handle_sidecar_step(root, args, context.package_manager) {
        return exit;
    }

    let recipe_path = match write_recipe(root, &context) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("fallow coverage setup: {message}");
            return ExitCode::from(2);
        }
    };

    if let Some(coverage_path) = detect_coverage_artifact(root) {
        println!(
            "Step 3/4: Coverage found at {}",
            display_relative(root, &coverage_path)
        );
        println!(
            "Step 4/4: Running fallow health --production-coverage {} ...",
            display_relative(root, &coverage_path)
        );
        let exit = run_health_analysis(root, &coverage_path);
        print_upload_inventory_hint();
        return exit;
    }

    println!("Step 3/4: Collecting coverage for your app.");
    println!("  -> Detected: {}.", context.framework.label());
    println!(
        "  -> Wrote {} with the {} recipe.",
        display_relative(root, &recipe_path),
        context.framework.label()
    );
    println!("  -> Run your app with the instrumentation on, then re-run this command.");
    print_upload_inventory_hint();
    ExitCode::SUCCESS
}

/// Nudge the user toward `fallow coverage upload-inventory`. The runtime
/// beacon gives the dashboard `called` / `never_called`; the static inventory
/// upload gives it `untracked` (functions that exist but runtime never parsed).
/// Without this hint, trial users finish setup with no signal that the
/// dashboard's Untracked filter needs a second CI step to light up.
fn print_upload_inventory_hint() {
    println!();
    println!("Next, in CI, upload the static function inventory so the dashboard's");
    println!("Untracked filter lights up:");
    println!("  fallow coverage upload-inventory");
    println!("Set FALLOW_API_KEY on the runner. See {COVERAGE_DOCS_URL} for the full CI snippet.");
}

fn handle_license_step(
    root: &Path,
    args: SetupArgs,
    license_state: &Result<LicenseStatus, fallow_license::LicenseError>,
) -> Option<ExitCode> {
    match license_state {
        Ok(
            LicenseStatus::Valid { .. }
            | LicenseStatus::ExpiredWarning { .. }
            | LicenseStatus::ExpiredWatermark { .. },
        ) => {
            println!("Step 1/4: License check... ok.");
            None
        }
        Ok(LicenseStatus::Missing) => {
            println!("Step 1/4: License check... none found.");
            start_trial_if_needed(root, args)
        }
        Ok(LicenseStatus::HardFail {
            days_since_expiry, ..
        }) => {
            println!("Step 1/4: License check... expired {days_since_expiry} days ago.");
            start_trial_if_needed(root, args)
        }
        Err(err) => {
            println!("Step 1/4: License check... existing token is invalid ({err}).");
            start_trial_if_needed(root, args)
        }
    }
}

fn start_trial_if_needed(root: &Path, args: SetupArgs) -> Option<ExitCode> {
    let prompt = "  -> Start a 30-day trial (email only, no card)? [Y/n] ";
    let accepted = match confirm(prompt, args) {
        Ok(accepted) => accepted,
        Err(message) => {
            eprintln!("fallow coverage setup: {message}");
            return Some(ExitCode::from(2));
        }
    };
    if !accepted {
        println!("  -> Run: fallow license activate --trial --email you@company.com");
        return Some(ExitCode::SUCCESS);
    }

    let email = match prompt_email(args) {
        Ok(Some(email)) => email,
        Ok(None) => return Some(ExitCode::SUCCESS),
        Err(message) => {
            eprintln!("fallow coverage setup: {message}");
            return Some(ExitCode::from(2));
        }
    };

    match license::activate_trial(&email) {
        Ok(status) => {
            println!(
                "  -> This license is machine-scoped (stored at {}).",
                default_license_display(root)
            );
            println!("     Your teammates each start their own trial.");
            print_trial_status(&status);
            None
        }
        Err(message) => {
            eprintln!("fallow coverage setup: {message}");
            Some(ExitCode::from(7))
        }
    }
}

fn handle_sidecar_step(
    root: &Path,
    args: SetupArgs,
    package_manager: Option<PackageManager>,
) -> Option<ExitCode> {
    match production_coverage::discover_sidecar(Some(root)) {
        Ok(path) => {
            println!("Step 2/4: Sidecar check... ok ({})", path.to_string_lossy());
            None
        }
        Err(message) => {
            println!("Step 2/4: Sidecar check... not installed.");
            println!("  -> {message}");
            let install_command = package_manager.map_or_else(
                || "npm install -g @fallow-cli/fallow-cov".to_owned(),
                PackageManager::install_command,
            );
            let prompt = if let Some(package_manager) = package_manager {
                format!(
                    "  -> Install @fallow-cli/fallow-cov with {}? [Y/n] ",
                    package_manager.label()
                )
            } else {
                "  -> Install @fallow-cli/fallow-cov globally via npm? [Y/n] ".to_owned()
            };
            let accepted = match confirm(prompt, args) {
                Ok(accepted) => accepted,
                Err(message) => {
                    eprintln!("fallow coverage setup: {message}");
                    return Some(ExitCode::from(2));
                }
            };
            if !accepted {
                println!("  -> Run: {install_command}");
                println!(
                    "  -> Manual fallback: install a signed binary and place it at {}",
                    production_coverage::canonical_sidecar_path().display()
                );
                return Some(ExitCode::SUCCESS);
            }

            match install_sidecar(root, package_manager) {
                Ok(path) => {
                    println!("  -> Installed at {}", path.display());
                    None
                }
                Err(message) => {
                    eprintln!("fallow coverage setup: {message}");
                    Some(ExitCode::from(4))
                }
            }
        }
    }
}

fn confirm(prompt: impl AsRef<str>, args: SetupArgs) -> Result<bool, String> {
    let prompt = prompt.as_ref();
    if args.non_interactive {
        println!("{prompt}skipped (--non-interactive)");
        return Ok(false);
    }
    if args.yes {
        println!("{prompt}Y");
        return Ok(true);
    }

    print!("{prompt}");
    io::stdout()
        .flush()
        .map_err(|err| format!("failed to flush stdout: {err}"))?;

    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|err| format!("failed to read stdin: {err}"))?;
    let trimmed = answer.trim().to_ascii_lowercase();
    Ok(trimmed.is_empty() || trimmed == "y" || trimmed == "yes")
}

fn prompt_email(args: SetupArgs) -> Result<Option<String>, String> {
    if args.non_interactive {
        println!("  -> Run: fallow license activate --trial --email you@company.com");
        return Ok(None);
    }
    if args.yes {
        let Some(email) = default_trial_email() else {
            return Err(
                "unable to infer an email address for --yes. Run without --yes or use `fallow license activate --trial --email <addr>` first."
                    .to_owned(),
            );
        };
        println!("  -> Email: {email}");
        return Ok(Some(email));
    }

    print!("  -> Email: ");
    io::stdout()
        .flush()
        .map_err(|err| format!("failed to flush stdout: {err}"))?;

    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|err| format!("failed to read stdin: {err}"))?;
    let trimmed = answer.trim();
    if trimmed.is_empty() {
        return Err("email is required to start a trial".to_owned());
    }
    Ok(Some(trimmed.to_owned()))
}

fn default_trial_email() -> Option<String> {
    std::env::var("EMAIL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(git_config_email)
}

fn git_config_email() -> Option<String> {
    let output = Command::new("git")
        .args(["config", "user.email"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let email = String::from_utf8(output.stdout).ok()?;
    let trimmed = email.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn print_trial_status(status: &LicenseStatus) {
    match status {
        LicenseStatus::Valid {
            days_until_expiry, ..
        } => {
            println!("  -> Trial active. {days_until_expiry} days remaining.");
        }
        LicenseStatus::ExpiredWarning {
            days_since_expiry, ..
        }
        | LicenseStatus::ExpiredWatermark {
            days_since_expiry, ..
        }
        | LicenseStatus::HardFail {
            days_since_expiry, ..
        } => {
            println!(
                "  -> Trial activated, but it is already expired by {days_since_expiry} days."
            );
        }
        LicenseStatus::Missing => {
            println!("  -> Trial request completed, but no license was stored.");
        }
    }
}

fn default_license_display(root: &Path) -> String {
    display_relative(root, &fallow_license::default_license_path())
}

fn install_sidecar(
    root: &Path,
    package_manager: Option<PackageManager>,
) -> Result<PathBuf, String> {
    let (program, args, current_dir, display_command) =
        if let Some(package_manager) = package_manager {
            let (program, args) = package_manager.install_args();
            (program, args, root, package_manager.install_command())
        } else {
            (
                "npm",
                &["install", "-g", "@fallow-cli/fallow-cov"][..],
                root,
                "npm install -g @fallow-cli/fallow-cov".to_owned(),
            )
        };

    let status = Command::new(program)
        .args(args)
        .current_dir(current_dir)
        .status()
        .map_err(|err| format!("failed to run {display_command}: {err}"))?;

    if !status.success() {
        return Err(format!(
            "{display_command} failed. Install it manually or place the binary in {}",
            production_coverage::canonical_sidecar_path().display()
        ));
    }

    production_coverage::discover_sidecar(Some(root)).map_err(|_| {
        format!(
            "sidecar install finished but fallow still could not find fallow-cov. Checked project-local node_modules/.bin, {}, and PATH",
            production_coverage::canonical_sidecar_path().display()
        )
    })
}

fn detect_setup_context(root: &Path) -> CoverageSetupContext {
    let package_json = PackageJson::load(&root.join("package.json")).ok();
    let framework = detect_framework(package_json.as_ref());
    let package_manager = detect_package_manager(root);
    let scripts = package_json.as_ref().and_then(|pkg| pkg.scripts.as_ref());
    CoverageSetupContext {
        framework,
        package_manager,
        has_build_script: scripts.is_some_and(|scripts| scripts.contains_key("build")),
        has_start_script: scripts.is_some_and(|scripts| scripts.contains_key("start")),
        has_preview_script: scripts.is_some_and(|scripts| scripts.contains_key("preview")),
    }
}

fn detect_framework(package_json: Option<&PackageJson>) -> FrameworkKind {
    let Some(package_json) = package_json else {
        return FrameworkKind::Other;
    };
    let dependencies = package_json.all_dependency_names();
    if dependencies.iter().any(|name| name == "next") {
        FrameworkKind::NextJs
    } else if dependencies.iter().any(|name| name.starts_with("@nestjs/")) {
        FrameworkKind::NestJs
    } else if dependencies
        .iter()
        .any(|name| name == "nuxt" || name == "nuxi")
    {
        FrameworkKind::Nuxt
    } else if dependencies.iter().any(|name| name == "@sveltejs/kit") {
        FrameworkKind::SvelteKit
    } else if dependencies.iter().any(|name| name == "astro") {
        FrameworkKind::Astro
    } else if dependencies
        .iter()
        .any(|name| name == "remix" || name.starts_with("@remix-run/"))
    {
        FrameworkKind::Remix
    } else if package_json.name.is_some() {
        FrameworkKind::PlainNode
    } else {
        FrameworkKind::Other
    }
}

fn detect_package_manager(root: &Path) -> Option<PackageManager> {
    detect_package_manager_from_field(root).or_else(|| {
        if root.join("bun.lockb").exists() || root.join("bun.lock").exists() {
            Some(PackageManager::Bun)
        } else if root.join("pnpm-lock.yaml").exists() {
            Some(PackageManager::Pnpm)
        } else if root.join("yarn.lock").exists() {
            Some(PackageManager::Yarn)
        } else if root.join("package-lock.json").exists()
            || root.join("npm-shrinkwrap.json").exists()
        {
            Some(PackageManager::Npm)
        } else {
            None
        }
    })
}

fn detect_package_manager_from_field(root: &Path) -> Option<PackageManager> {
    let content = std::fs::read_to_string(root.join("package.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    let field = value.get("packageManager")?.as_str()?;
    let name = field.split('@').next().unwrap_or(field);
    match name {
        "npm" => Some(PackageManager::Npm),
        "pnpm" => Some(PackageManager::Pnpm),
        "yarn" => Some(PackageManager::Yarn),
        "bun" => Some(PackageManager::Bun),
        _ => None,
    }
}

fn write_recipe(root: &Path, context: &CoverageSetupContext) -> Result<PathBuf, String> {
    let docs_dir = root.join("docs");
    std::fs::create_dir_all(&docs_dir)
        .map_err(|err| format!("failed to create {}: {err}", docs_dir.display()))?;
    let path = docs_dir.join("collect-coverage.md");
    std::fs::write(&path, recipe_contents(context))
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(path)
}

fn recipe_contents(context: &CoverageSetupContext) -> String {
    let title = match context.framework {
        FrameworkKind::NextJs => "Next.js",
        FrameworkKind::NestJs => "NestJS",
        FrameworkKind::Nuxt => "Nuxt",
        FrameworkKind::SvelteKit => "SvelteKit",
        FrameworkKind::Astro => "Astro",
        FrameworkKind::Remix => "Remix",
        FrameworkKind::PlainNode => "Node service",
        FrameworkKind::Other => {
            return format!(
                "# Collect production coverage\n\nThis project was not matched to a built-in recipe.\nSee {COVERAGE_DOCS_URL} for framework-specific instructions.\n"
            );
        }
    };

    let mut lines = vec![
        format!("# Collect production coverage for {title}"),
        String::new(),
    ];
    lines.push("1. Remove any old dump directory: `rm -rf ./coverage`".to_owned());
    let final_step = if context.has_build_script || context.build_command().is_some() {
        if let Some(build_command) = context.build_command() {
            lines.push(format!("2. Build the app: `{build_command}`"));
        }
        lines.push(format!(
            "3. Start the app with V8 coverage enabled: `NODE_V8_COVERAGE=./coverage {}`",
            context.run_command()
        ));
        lines.push("4. Exercise the routes or jobs you care about.".to_owned());
        lines.push("5. Stop the app and run: `fallow coverage setup`".to_owned());
        "6"
    } else {
        lines.push(format!(
            "2. Start the app with V8 coverage enabled: `NODE_V8_COVERAGE=./coverage {}`",
            context.run_command()
        ));
        lines.push("3. Exercise the app traffic you want to analyze.".to_owned());
        lines.push("4. Stop the process and run: `fallow coverage setup`".to_owned());
        "5"
    };
    lines.push(format!(
        "{final_step}. In CI, after the build, run \
         `fallow coverage upload-inventory` with `FALLOW_API_KEY` set. The \
         upload is what enables the dashboard's Untracked filter (functions \
         that exist but runtime coverage never parsed). Runtime coverage alone \
         only answers `called` vs `never_called`; the static inventory adds \
         the third state."
    ));
    lines.push(String::new());
    lines.join("\n")
}

fn detect_coverage_artifact(root: &Path) -> Option<PathBuf> {
    for file in [
        root.join("coverage/coverage-final.json"),
        root.join(".nyc_output/coverage-final.json"),
    ] {
        if file.is_file() {
            return Some(file);
        }
    }

    [root.join("coverage"), root.join(".nyc_output")]
        .into_iter()
        .find(|dir| dir.is_dir() && directory_has_json(dir))
}

fn directory_has_json(path: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };

    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .any(|entry| entry.extension() == Some(OsStr::new("json")))
}

fn run_health_analysis(root: &Path, coverage_path: &Path) -> ExitCode {
    let current_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("fallow coverage setup: failed to resolve current executable: {err}");
            return ExitCode::from(2);
        }
    };

    let status = match Command::new(current_exe)
        .arg("health")
        .arg("--root")
        .arg(root)
        .arg("--production-coverage")
        .arg(coverage_path)
        .status()
    {
        Ok(status) => status,
        Err(err) => {
            eprintln!("fallow coverage setup: failed to run health analysis: {err}");
            return ExitCode::from(2);
        }
    };

    match status.code() {
        Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(2)),
        None => ExitCode::from(2),
    }
}

fn display_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).map_or_else(
        |_| path.to_string_lossy().into_owned(),
        |relative| format!("./{}", relative.to_string_lossy()),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        CoverageSetupContext, FrameworkKind, PackageManager, detect_coverage_artifact,
        detect_framework, detect_package_manager, recipe_contents,
    };
    use fallow_config::PackageJson;
    use tempfile::tempdir;

    #[test]
    fn detect_framework_recognizes_nuxt_projects() {
        let package_json: PackageJson =
            serde_json::from_str(r#"{"name":"demo","dependencies":{"nuxt":"^3.0.0"}}"#)
                .expect("package.json should parse");

        assert_eq!(detect_framework(Some(&package_json)), FrameworkKind::Nuxt);
    }

    #[test]
    fn detect_package_manager_prefers_package_manager_field() {
        let dir = tempdir().expect("tempdir should be created");
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"demo","packageManager":"bun@1.2.0"}"#,
        )
        .expect("package.json should be written");
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "lockfileVersion: '9.0'")
            .expect("lockfile should be written");

        assert_eq!(
            detect_package_manager(dir.path()),
            Some(PackageManager::Bun)
        );
    }

    #[test]
    fn recipe_contents_uses_detected_package_manager_scripts() {
        let context = CoverageSetupContext {
            framework: FrameworkKind::SvelteKit,
            package_manager: Some(PackageManager::Pnpm),
            has_build_script: true,
            has_start_script: false,
            has_preview_script: true,
        };

        let recipe = recipe_contents(&context);

        assert!(recipe.contains("`pnpm build`"));
        assert!(recipe.contains("`NODE_V8_COVERAGE=./coverage pnpm preview`"));
    }

    #[test]
    fn recipe_contents_mentions_upload_inventory_ci_step() {
        let context = CoverageSetupContext {
            framework: FrameworkKind::SvelteKit,
            package_manager: Some(PackageManager::Pnpm),
            has_build_script: true,
            has_start_script: false,
            has_preview_script: true,
        };
        let recipe = recipe_contents(&context);
        // Without this line the trial user finishes setup, wires the beacon,
        // and has no idea the dashboard's Untracked filter needs a second
        // CI step. Regression test for BLOCK 2 from the public-readiness
        // panel (2026-04-22).
        assert!(
            recipe.contains("fallow coverage upload-inventory"),
            "recipe missing upload-inventory CI instruction:\n{recipe}"
        );
        assert!(recipe.contains("FALLOW_API_KEY"));
    }

    #[test]
    fn recipe_contents_mentions_upload_inventory_without_build_script() {
        let context = CoverageSetupContext {
            framework: FrameworkKind::PlainNode,
            package_manager: Some(PackageManager::Npm),
            has_build_script: false,
            has_start_script: false,
            has_preview_script: false,
        };
        let recipe = recipe_contents(&context);
        assert!(recipe.contains("fallow coverage upload-inventory"));
    }

    #[test]
    fn detect_coverage_artifact_finds_nyc_output_istanbul_file() {
        let dir = tempdir().expect("tempdir should be created");
        let nyc_dir = dir.path().join(".nyc_output");
        std::fs::create_dir_all(&nyc_dir).expect("nyc dir should be created");
        let coverage_file = nyc_dir.join("coverage-final.json");
        std::fs::write(&coverage_file, "{}").expect("coverage file should be written");

        assert_eq!(detect_coverage_artifact(dir.path()), Some(coverage_file));
    }
}
