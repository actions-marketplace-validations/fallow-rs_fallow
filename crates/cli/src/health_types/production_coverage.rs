use std::fmt;
use std::path::PathBuf;

/// Top-level verdict for the whole production-coverage report. Mirrors
/// `fallow_cov_protocol::ReportVerdict`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductionCoverageReportVerdict {
    Clean,
    HotPathChangesNeeded,
    ColdCodeDetected,
    LicenseExpiredGrace,
    #[default]
    Unknown,
}

impl ProductionCoverageReportVerdict {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::HotPathChangesNeeded => "hot-path-changes-needed",
            Self::ColdCodeDetected => "cold-code-detected",
            Self::LicenseExpiredGrace => "license-expired-grace",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ProductionCoverageReportVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per-finding verdict. Replaces the 0.1 `state` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionCoverageVerdict {
    SafeToDelete,
    ReviewRequired,
    CoverageUnavailable,
    LowTraffic,
    Active,
    Unknown,
}

impl ProductionCoverageVerdict {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SafeToDelete => "safe_to_delete",
            Self::ReviewRequired => "review_required",
            Self::CoverageUnavailable => "coverage_unavailable",
            Self::LowTraffic => "low_traffic",
            Self::Active => "active",
            Self::Unknown => "unknown",
        }
    }

    #[must_use]
    pub const fn human_label(self) -> &'static str {
        match self {
            Self::SafeToDelete => "safe to delete",
            Self::ReviewRequired => "review required",
            Self::CoverageUnavailable => "coverage unavailable",
            Self::LowTraffic => "low traffic",
            Self::Active => "active",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ProductionCoverageVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionCoverageConfidence {
    VeryHigh,
    High,
    Medium,
    Low,
    None,
    Unknown,
}

impl ProductionCoverageConfidence {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VeryHigh => "very_high",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::None => "none",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ProductionCoverageConfidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductionCoverageWatermark {
    TrialExpired,
    LicenseExpiredGrace,
    Unknown,
}

impl ProductionCoverageWatermark {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TrialExpired => "trial-expired",
            Self::LicenseExpiredGrace => "license-expired-grace",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ProductionCoverageWatermark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Summary block mirroring `fallow_cov_protocol::Summary` (0.3 shape).
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ProductionCoverageSummary {
    pub functions_tracked: usize,
    pub functions_hit: usize,
    pub functions_unhit: usize,
    pub functions_untracked: usize,
    pub coverage_percent: f64,
    pub trace_count: u64,
    pub period_days: u32,
    pub deployments_seen: u32,
    /// Capture-quality telemetry. `None` for protocol-0.2 sidecars; protocol-0.3+
    /// sidecars always populate it. Fuels the human-output short-window warning
    /// and the quantified trial CTA, and is passed through to JSON consumers so
    /// agent pipelines can surface the same signal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_quality: Option<ProductionCoverageCaptureQuality>,
}

/// Capture-quality telemetry (mirrors `fallow_cov_protocol::CaptureQuality`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProductionCoverageCaptureQuality {
    pub window_seconds: u64,
    pub instances_observed: u32,
    pub lazy_parse_warning: bool,
    pub untracked_ratio_percent: f64,
}

/// Supporting evidence for a finding (mirrors `fallow_cov_protocol::Evidence`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductionCoverageEvidence {
    pub static_status: String,
    pub test_coverage: String,
    pub v8_tracking: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub untracked_reason: Option<String>,
    pub observation_days: u32,
    pub deployments_observed: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductionCoverageAction {
    /// Stable action identifier. Serialized as `type` in JSON to match the
    /// `actions[].type` contract shared with every other `fallow health` finding.
    #[serde(rename = "type")]
    pub kind: String,
    pub description: String,
    pub auto_fixable: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductionCoverageMessage {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductionCoverageFinding {
    /// Stable content-hash ID of the form `fallow:prod:<hash>`.
    pub id: String,
    pub path: PathBuf,
    pub function: String,
    pub line: u32,
    pub verdict: ProductionCoverageVerdict,
    /// Raw V8 invocation count. `None` when the function was untracked
    /// (lazy-parsed, worker thread, or dynamic code).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invocations: Option<u64>,
    pub confidence: ProductionCoverageConfidence,
    pub evidence: ProductionCoverageEvidence,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ProductionCoverageAction>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductionCoverageHotPath {
    /// Stable content-hash ID of the form `fallow:hot:<hash>`.
    pub id: String,
    pub path: PathBuf,
    pub function: String,
    pub line: u32,
    pub invocations: u64,
    /// Percentile rank over this response's hot-path distribution. `100`
    /// means the busiest, `0` means the quietest function that qualified.
    pub percentile: u8,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ProductionCoverageAction>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ProductionCoverageReport {
    pub verdict: ProductionCoverageReportVerdict,
    pub summary: ProductionCoverageSummary,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<ProductionCoverageFinding>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hot_paths: Vec<ProductionCoverageHotPath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watermark: Option<ProductionCoverageWatermark>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<ProductionCoverageMessage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_verdict_display_matches_kebab_case_serde() {
        assert_eq!(ProductionCoverageReportVerdict::Clean.to_string(), "clean");
        assert_eq!(
            ProductionCoverageReportVerdict::HotPathChangesNeeded.to_string(),
            "hot-path-changes-needed",
        );
        assert_eq!(
            ProductionCoverageReportVerdict::ColdCodeDetected.to_string(),
            "cold-code-detected",
        );
        assert_eq!(
            ProductionCoverageReportVerdict::LicenseExpiredGrace.to_string(),
            "license-expired-grace",
        );
        assert_eq!(
            ProductionCoverageReportVerdict::Unknown.to_string(),
            "unknown",
        );
    }

    #[test]
    fn verdict_display_matches_snake_case_serde() {
        assert_eq!(
            ProductionCoverageVerdict::SafeToDelete.to_string(),
            "safe_to_delete",
        );
        assert_eq!(
            ProductionCoverageVerdict::ReviewRequired.to_string(),
            "review_required",
        );
        assert_eq!(
            ProductionCoverageVerdict::CoverageUnavailable.to_string(),
            "coverage_unavailable",
        );
        assert_eq!(
            ProductionCoverageVerdict::LowTraffic.to_string(),
            "low_traffic",
        );
        assert_eq!(ProductionCoverageVerdict::Active.to_string(), "active");
    }

    #[test]
    fn confidence_display_matches_snake_case_serde() {
        assert_eq!(
            ProductionCoverageConfidence::VeryHigh.to_string(),
            "very_high",
        );
        assert_eq!(ProductionCoverageConfidence::High.to_string(), "high");
        assert_eq!(ProductionCoverageConfidence::Medium.to_string(), "medium");
        assert_eq!(ProductionCoverageConfidence::Low.to_string(), "low");
        assert_eq!(ProductionCoverageConfidence::None.to_string(), "none");
        assert_eq!(ProductionCoverageConfidence::Unknown.to_string(), "unknown");
    }

    #[test]
    fn watermark_display_matches_kebab_case_serde() {
        assert_eq!(
            ProductionCoverageWatermark::TrialExpired.to_string(),
            "trial-expired",
        );
        assert_eq!(
            ProductionCoverageWatermark::LicenseExpiredGrace.to_string(),
            "license-expired-grace",
        );
    }

    #[test]
    fn action_serializes_kind_as_type() {
        let action = ProductionCoverageAction {
            kind: "review-deletion".to_owned(),
            description: "Remove the function.".to_owned(),
            auto_fixable: false,
        };
        let value = serde_json::to_value(&action).expect("action should serialize");
        assert_eq!(value["type"], "review-deletion");
        assert!(
            value.get("kind").is_none(),
            "kind should be renamed to type"
        );
    }
}
