use std::process::ExitCode;
use std::time::Instant;

use fallow_config::OutputFormat;

use crate::baseline::{HealthBaselineData, filter_new_health_findings};
use crate::check::{get_changed_files, resolve_workspace_filter};
pub use crate::health_types::*;
use crate::load_config;
use crate::report;

/// Sort criteria for complexity output.
#[derive(Clone, clap::ValueEnum)]
pub enum SortBy {
    Cyclomatic,
    Cognitive,
    Lines,
}

pub struct HealthOptions<'a> {
    pub root: &'a std::path::Path,
    pub config_path: &'a Option<std::path::PathBuf>,
    pub output: OutputFormat,
    pub no_cache: bool,
    pub threads: usize,
    pub quiet: bool,
    pub max_cyclomatic: Option<u16>,
    pub max_cognitive: Option<u16>,
    pub top: Option<usize>,
    pub sort: SortBy,
    pub production: bool,
    pub changed_since: Option<&'a str>,
    pub workspace: Option<&'a str>,
    pub baseline: Option<&'a std::path::Path>,
    pub save_baseline: Option<&'a std::path::Path>,
    pub file_scores: bool,
}

pub fn run_health(opts: &HealthOptions<'_>) -> ExitCode {
    let start = Instant::now();

    let config = match load_config(
        opts.root,
        opts.config_path,
        opts.output.clone(),
        opts.no_cache,
        opts.threads,
        opts.production,
        opts.quiet,
    ) {
        Ok(c) => c,
        Err(code) => return code,
    };

    // Resolve thresholds: CLI flags override config
    let max_cyclomatic = opts.max_cyclomatic.unwrap_or(config.health.max_cyclomatic);
    let max_cognitive = opts.max_cognitive.unwrap_or(config.health.max_cognitive);

    // Discover files
    let files = fallow_core::discover::discover_files(&config);

    // Parse all files (complexity is computed during parsing)
    let cache = if config.no_cache {
        None
    } else {
        fallow_core::cache::CacheStore::load(&config.cache_dir)
    };
    let parse_result = fallow_core::extract::parse_all_files(&files, cache.as_ref());

    // Build ignore globs from config (using globset for consistency with the rest of the codebase)
    let ignore_set = {
        let mut builder = globset::GlobSetBuilder::new();
        for pattern in &config.health.ignore {
            match globset::Glob::new(pattern) {
                Ok(glob) => {
                    builder.add(glob);
                }
                Err(e) => {
                    eprintln!("Warning: Invalid health ignore pattern '{pattern}': {e}");
                }
            }
        }
        builder
            .build()
            .unwrap_or_else(|_| globset::GlobSet::empty())
    };

    // Get changed files for --changed-since filtering
    let changed_files = opts
        .changed_since
        .and_then(|git_ref| get_changed_files(opts.root, git_ref));

    // Resolve workspace filter once — reused for both findings and file scores
    let ws_root = if let Some(ws_name) = opts.workspace {
        match resolve_workspace_filter(opts.root, ws_name, &opts.output) {
            Ok(root) => Some(root),
            Err(code) => return code,
        }
    } else {
        None
    };

    // Build FileId → path lookup for O(1) access
    let file_paths: rustc_hash::FxHashMap<_, _> = files.iter().map(|f| (f.id, &f.path)).collect();

    // Collect findings
    let mut files_analyzed = 0usize;
    let mut total_functions = 0usize;
    let mut findings: Vec<HealthFinding> = Vec::new();

    for module in &parse_result.modules {
        let Some(path) = file_paths.get(&module.file_id) else {
            continue;
        };

        // Apply ignore patterns
        let relative = path.strip_prefix(&config.root).unwrap_or(path);
        if ignore_set.is_match(relative) {
            continue;
        }

        // Apply changed-since filter
        if let Some(ref changed) = changed_files
            && !changed.contains(*path)
        {
            continue;
        }

        files_analyzed += 1;
        for fc in &module.complexity {
            total_functions += 1;
            let exceeds_cyclomatic = fc.cyclomatic > max_cyclomatic;
            let exceeds_cognitive = fc.cognitive > max_cognitive;
            if exceeds_cyclomatic || exceeds_cognitive {
                let exceeded = match (exceeds_cyclomatic, exceeds_cognitive) {
                    (true, true) => ExceededThreshold::Both,
                    (true, false) => ExceededThreshold::Cyclomatic,
                    (false, true) => ExceededThreshold::Cognitive,
                    (false, false) => unreachable!(),
                };
                findings.push(HealthFinding {
                    path: (*path).clone(),
                    name: fc.name.clone(),
                    line: fc.line,
                    col: fc.col,
                    cyclomatic: fc.cyclomatic,
                    cognitive: fc.cognitive,
                    line_count: fc.line_count,
                    exceeded,
                });
            }
        }
    }

    // Apply workspace filter (resolved once above, reused for file scores too)
    if let Some(ref ws) = ws_root {
        findings.retain(|f| f.path.starts_with(ws));
    }

    // Sort findings
    match opts.sort {
        SortBy::Cyclomatic => findings.sort_by(|a, b| b.cyclomatic.cmp(&a.cyclomatic)),
        SortBy::Cognitive => findings.sort_by(|a, b| b.cognitive.cmp(&a.cognitive)),
        SortBy::Lines => findings.sort_by(|a, b| b.line_count.cmp(&a.line_count)),
    }

    // Save baseline (before filtering, captures full state)
    if let Some(save_path) = opts.save_baseline {
        let baseline = HealthBaselineData::from_findings(&findings, &config.root);
        match serde_json::to_string_pretty(&baseline) {
            Ok(json) => {
                if let Err(e) = std::fs::write(save_path, json) {
                    eprintln!("Error: failed to save health baseline: {e}");
                    return ExitCode::from(2);
                }
                if !opts.quiet {
                    eprintln!("Saved health baseline to {}", save_path.display());
                }
            }
            Err(e) => {
                eprintln!("Error: failed to serialize health baseline: {e}");
                return ExitCode::from(2);
            }
        }
    }

    // Capture total above threshold before baseline filtering
    let total_above_threshold = findings.len();

    // Filter against baseline
    if let Some(load_path) = opts.baseline {
        match std::fs::read_to_string(load_path) {
            Ok(json) => match serde_json::from_str::<HealthBaselineData>(&json) {
                Ok(baseline) => {
                    findings = filter_new_health_findings(findings, &baseline, &config.root);
                }
                Err(e) => {
                    eprintln!("Error: failed to parse health baseline: {e}");
                    return ExitCode::from(2);
                }
            },
            Err(e) => {
                eprintln!("Error: failed to read health baseline: {e}");
                return ExitCode::from(2);
            }
        }
    }

    // Apply --top limit
    if let Some(top) = opts.top {
        findings.truncate(top);
    }

    // Compute file-level health scores when requested.
    // NOTE: This runs the full analysis pipeline (discovery, parsing, graph, dead code detection)
    // a second time because there is no API to inject pre-parsed modules into the analysis
    // pipeline. The cache mitigates re-parsing cost but the discovery and graph construction
    // are repeated. Future optimization: expose a lower-level API that accepts ParseResult.
    let (file_scores, files_scored, average_maintainability) = if opts.file_scores {
        match compute_file_scores(
            &config,
            &parse_result.modules,
            &file_paths,
            changed_files.as_ref(),
        ) {
            Ok(mut scores) => {
                // Apply the same filters that findings get: workspace, ignore globs
                if let Some(ref ws) = ws_root {
                    scores.retain(|s| s.path.starts_with(ws));
                }
                if !ignore_set.is_empty() {
                    scores.retain(|s| {
                        let relative = s.path.strip_prefix(&config.root).unwrap_or(&s.path);
                        !ignore_set.is_match(relative)
                    });
                }
                // Apply --top to file scores too
                if let Some(top) = opts.top {
                    scores.truncate(top);
                }
                let count = scores.len();
                let avg = if count > 0 {
                    let sum: f64 = scores.iter().map(|s| s.maintainability_index).sum();
                    Some((sum / count as f64 * 10.0).round() / 10.0)
                } else {
                    None
                };
                (scores, Some(count), avg)
            }
            Err(e) => {
                eprintln!("Warning: failed to compute file scores: {e}");
                (Vec::new(), None, None)
            }
        }
    } else {
        (Vec::new(), None, None)
    };

    let report = HealthReport {
        summary: HealthSummary {
            files_analyzed,
            functions_analyzed: total_functions,
            functions_above_threshold: total_above_threshold,
            max_cyclomatic_threshold: max_cyclomatic,
            max_cognitive_threshold: max_cognitive,
            files_scored,
            average_maintainability,
        },
        findings,
        file_scores,
    };

    let elapsed = start.elapsed();

    // Print report
    let result = report::print_health_report(&report, &config, elapsed, opts.quiet, &opts.output);
    if result != ExitCode::SUCCESS {
        return result;
    }

    // Exit code 1 if there are findings
    if !report.findings.is_empty() {
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}

/// Compute per-file health scores by running the full analysis pipeline.
///
/// This builds the module graph and runs dead code detection to obtain
/// fan-in, fan-out, and dead code ratio per file. Complexity density is
/// derived from the already-parsed modules.
fn compute_file_scores(
    config: &fallow_config::ResolvedConfig,
    modules: &[fallow_core::extract::ModuleInfo],
    file_paths: &rustc_hash::FxHashMap<fallow_core::discover::FileId, &std::path::PathBuf>,
    changed_files: Option<&rustc_hash::FxHashSet<std::path::PathBuf>>,
) -> Result<Vec<FileHealthScore>, String> {
    // Run full analysis to get the graph and dead code results
    let output = fallow_core::analyze_with_trace(config).map_err(|e| format!("{e}"))?;
    let graph = output.graph.ok_or("graph not available")?;
    let results = &output.results;

    // Build a set of unused file paths for O(1) lookup
    let unused_files: rustc_hash::FxHashSet<&std::path::Path> = results
        .unused_files
        .iter()
        .map(|f| f.path.as_path())
        .collect();

    // Count unused exports per file path
    let mut unused_exports_by_path: rustc_hash::FxHashMap<&std::path::Path, usize> =
        rustc_hash::FxHashMap::default();
    for exp in &results.unused_exports {
        *unused_exports_by_path
            .entry(exp.path.as_path())
            .or_default() += 1;
    }
    for exp in &results.unused_types {
        *unused_exports_by_path
            .entry(exp.path.as_path())
            .or_default() += 1;
    }

    // Build FileId → ModuleInfo lookup
    let module_by_id: rustc_hash::FxHashMap<
        fallow_core::discover::FileId,
        &fallow_core::extract::ModuleInfo,
    > = modules.iter().map(|m| (m.file_id, m)).collect();

    let mut scores = Vec::with_capacity(graph.modules.len());

    for node in &graph.modules {
        let Some(path) = file_paths.get(&node.file_id) else {
            continue;
        };

        // Fan-in: number of files that import this file
        let fan_in = graph
            .reverse_deps
            .get(node.file_id.0 as usize)
            .map_or(0, Vec::len);

        // Fan-out: number of files this file imports (from edge_range)
        let fan_out = node.edge_range.len();

        // Get complexity data from parsed module
        let (total_cyclomatic, total_cognitive, function_count, lines) =
            if let Some(module) = module_by_id.get(&node.file_id) {
                let cyc: u32 = module
                    .complexity
                    .iter()
                    .map(|f| u32::from(f.cyclomatic))
                    .sum();
                let cog: u32 = module
                    .complexity
                    .iter()
                    .map(|f| u32::from(f.cognitive))
                    .sum();
                let funcs = module.complexity.len();
                // line_offsets length = number of lines in the file
                let line_count = module.line_offsets.len() as u32;
                (cyc, cog, funcs, line_count)
            } else {
                (0, 0, 0, 0)
            };

        // Dead code ratio: fraction of exports with zero references.
        // If the entire file is unused, ratio is 1.0.
        let dead_code_ratio = if unused_files.contains((*path).as_path()) {
            1.0
        } else {
            let total_exports = node.exports.len();
            if total_exports > 0 {
                let unused = unused_exports_by_path
                    .get(path.as_path())
                    .copied()
                    .unwrap_or(0);
                (unused as f64 / total_exports as f64).min(1.0)
            } else {
                0.0
            }
        };

        // Complexity density: total cyclomatic / lines of code
        let complexity_density = if lines > 0 {
            f64::from(total_cyclomatic) / f64::from(lines)
        } else {
            0.0
        };

        // Round intermediate values first so the MI in JSON is reproducible
        // from the other rounded fields in the same JSON object.
        let dead_code_ratio_rounded = (dead_code_ratio * 100.0).round() / 100.0;
        let complexity_density_rounded = (complexity_density * 100.0).round() / 100.0;

        // Maintainability index:
        // 100 - (complexity_density × 30) - (dead_code_ratio × 20) - (fan_out × 0.5)
        // Clamped to [0, 100].
        let maintainability_index = compute_maintainability_index(
            complexity_density_rounded,
            dead_code_ratio_rounded,
            fan_out,
        );

        scores.push(FileHealthScore {
            path: (*path).clone(),
            fan_in,
            fan_out,
            dead_code_ratio: dead_code_ratio_rounded,
            complexity_density: complexity_density_rounded,
            maintainability_index: (maintainability_index * 10.0).round() / 10.0,
            total_cyclomatic,
            total_cognitive,
            function_count,
            lines,
        });
    }

    // Apply --changed-since filter to keep scores consistent with findings
    if let Some(changed) = changed_files {
        scores.retain(|s| changed.contains(&s.path));
    }

    // Sort by maintainability index ascending (worst files first)
    scores.sort_by(|a, b| {
        a.maintainability_index
            .partial_cmp(&b.maintainability_index)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(scores)
}

/// Compute the maintainability index for a single file.
///
/// Formula: `100 - (complexity_density × 30) - (dead_code_ratio × 20) - (fan_out × 0.5)`
///
/// Clamped to \[0, 100\]. Higher is better.
fn compute_maintainability_index(
    complexity_density: f64,
    dead_code_ratio: f64,
    fan_out: usize,
) -> f64 {
    // Keep the formula readable — it matches the documented specification.
    #[expect(clippy::suboptimal_flops)]
    let score =
        100.0 - (complexity_density * 30.0) - (dead_code_ratio * 20.0) - (fan_out as f64 * 0.5);
    score.clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maintainability_perfect_score() {
        // No complexity, no dead code, no fan-out → 100
        assert!((compute_maintainability_index(0.0, 0.0, 0) - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn maintainability_clamped_at_zero() {
        // Very high complexity density → clamped to 0
        assert!((compute_maintainability_index(10.0, 1.0, 100) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn maintainability_formula_correct() {
        // complexity_density=0.5, dead_code_ratio=0.3, fan_out=10
        // 100 - (0.5*30) - (0.3*20) - (10*0.5) = 100 - 15 - 6 - 5 = 74
        let result = compute_maintainability_index(0.5, 0.3, 10);
        assert!((result - 74.0).abs() < f64::EPSILON);
    }

    #[test]
    fn maintainability_dead_file_penalty() {
        // Fully dead file: dead_code_ratio=1.0
        // 100 - 0 - 20 - 0 = 80
        let result = compute_maintainability_index(0.0, 1.0, 0);
        assert!((result - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn maintainability_high_fan_out_penalty() {
        // fan_out=100: 100 - 0 - 0 - 50 = 50
        let result = compute_maintainability_index(0.0, 0.0, 100);
        assert!((result - 50.0).abs() < f64::EPSILON);
    }
}
