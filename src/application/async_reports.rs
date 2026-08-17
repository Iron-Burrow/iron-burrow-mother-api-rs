//! Closed report-type catalog. A companion accepted specification must add a
//! concrete entry before any public report can be requested.

use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReportDefinition {
    pub(crate) report_type: &'static str,
    pub(crate) version: i32,
}

const REPORTS: &[ReportDefinition] = &[];

pub(crate) fn lookup(report_type: &str, version: i32) -> Option<ReportDefinition> {
    REPORTS
        .iter()
        .copied()
        .find(|definition| definition.report_type == report_type && definition.version == version)
}

pub(crate) fn validate_input(_definition: ReportDefinition, input: &Value) -> bool {
    input.is_object()
}

pub(crate) fn validate_report(_definition: ReportDefinition, report: &Value) -> bool {
    report.is_object()
}
