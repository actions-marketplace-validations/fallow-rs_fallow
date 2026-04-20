use crate::params::HealthProductionCoverageParams;

use super::{push_global, push_scope};

/// Build CLI arguments for the `health_production_coverage` tool.
pub fn build_health_production_coverage_args(
    params: &HealthProductionCoverageParams,
) -> Vec<String> {
    let mut args = vec![
        "health".to_string(),
        "--format".to_string(),
        "json".to_string(),
        "--quiet".to_string(),
        "--explain".to_string(),
        "--production-coverage".to_string(),
        params.coverage.clone(),
    ];

    push_global(
        &mut args,
        params.root.as_deref(),
        params.config.as_deref(),
        params.no_cache,
        params.threads,
    );
    push_scope(&mut args, params.production, params.workspace.as_deref());

    if let Some(min_invocations_hot) = params.min_invocations_hot {
        args.extend([
            "--min-invocations-hot".to_string(),
            min_invocations_hot.to_string(),
        ]);
    }
    if let Some(min_observation_volume) = params.min_observation_volume {
        args.extend([
            "--min-observation-volume".to_string(),
            min_observation_volume.to_string(),
        ]);
    }
    if let Some(low_traffic_threshold) = params.low_traffic_threshold {
        args.extend([
            "--low-traffic-threshold".to_string(),
            low_traffic_threshold.to_string(),
        ]);
    }

    args
}
