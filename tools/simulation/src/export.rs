//! Metrics and report export
//!
//! Serializes SimMetrics and reports to JSON for external consumption.

use crate::engine::SimEvent;
use crate::metrics::SimMetrics;
use serde::{Deserialize, Serialize};

/// Combined export containing all simulation outputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationExport {
    pub version: String,
    pub metrics: SimMetrics,
    pub depth_json: Option<String>,
    pub slippage_json: Option<String>,
    pub profitability_json: Option<String>,
    pub event_count: usize,
}

/// Build a complete simulation export.
pub fn build_export(
    events: &[SimEvent],
    metrics: &SimMetrics,
    depth_json: Option<String>,
    slippage_json: Option<String>,
    profitability_json: Option<String>,
) -> SimulationExport {
    SimulationExport {
        version: crate::VERSION.to_string(),
        metrics: metrics.clone(),
        depth_json,
        slippage_json,
        profitability_json,
        event_count: events.len(),
    }
}

/// Export complete simulation data as JSON.
pub fn export_json(export: &SimulationExport) -> String {
    serde_json::to_string_pretty(export).unwrap_or_default()
}

/// Write export to a file path.
pub fn write_to_file(export: &SimulationExport, path: &str) -> std::io::Result<()> {
    let json = export_json(export);
    std::fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_export() {
        let metrics = SimMetrics::new();
        let export = build_export(&[], &metrics, None, None, None);
        assert_eq!(export.version, crate::VERSION);
        assert_eq!(export.event_count, 0);
    }

    #[test]
    fn test_export_json_roundtrip() {
        let metrics = SimMetrics::new();
        let export = build_export(&[], &metrics, None, None, None);
        let json = export_json(&export);
        let parsed: SimulationExport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, crate::VERSION);
    }

    #[test]
    fn test_export_with_reports() {
        let metrics = SimMetrics::new();
        let export = build_export(
            &[],
            &metrics,
            Some("{\"bids\":[]}".to_string()),
            Some("{\"records\":[]}".to_string()),
            Some("{\"accounts\":[]}".to_string()),
        );
        assert!(export.depth_json.is_some());
        assert!(export.slippage_json.is_some());
        assert!(export.profitability_json.is_some());
    }
}
