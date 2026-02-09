//! Security MCP tool handler implementations (9 tools).
//!
//! Contains the business logic for: scan_security, check_owasp, check_cwe,
//! explain_vulnerability, suggest_fix, find_injections, taint_sources,
//! security_summary, and trace_taint.

use std::path::Path;

use crate::security;

use super::server::json_text;

// 23. codegraph_scan_security
pub fn handle_scan_security(
    project_root: &Path,
    directory: Option<String>,
    exclude_tests: Option<bool>,
) -> String {
    let dir = match directory {
        Some(ref d) => match crate::observability::validate_path(d, project_root) {
            Ok(p) => p,
            Err(e) => return json_text(&serde_json::json!({"error": e})),
        },
        None => project_root.to_path_buf(),
    };
    let rules = security::rules::load_bundled_rules();
    let summary = security::scanner::scan_directory(&dir, &rules, exclude_tests.unwrap_or(true));
    json_text(&serde_json::json!({
        "totalFindings": summary.total_findings,
        "critical": summary.critical, "high": summary.high,
        "medium": summary.medium, "low": summary.low,
        "filesScanned": summary.files_scanned,
        "rulesApplied": summary.rules_applied,
        "topIssues": summary.top_issues.iter().map(|(name, count)| serde_json::json!({"rule": name, "count": count})).collect::<Vec<_>>(),
        "findings": summary.findings.iter().take(50).map(|f| serde_json::json!({
            "ruleId": f.rule_id, "ruleName": f.rule_name, "severity": format!("{:?}", f.severity),
            "file": f.file_path, "line": f.line_number, "message": f.message,
            "fix": f.fix, "cwe": f.cwe, "owasp": f.owasp,
        })).collect::<Vec<_>>(),
    }))
}

// 24. codegraph_check_owasp
pub fn handle_check_owasp(project_root: &Path, directory: Option<String>) -> String {
    let dir = match directory {
        Some(ref d) => match crate::observability::validate_path(d, project_root) {
            Ok(p) => p,
            Err(e) => return json_text(&serde_json::json!({"error": e})),
        },
        None => project_root.to_path_buf(),
    };
    let summary = security::scanner::check_owasp_top10(&dir);
    json_text(&serde_json::json!({
        "standard": "OWASP Top 10 2021",
        "totalFindings": summary.total_findings,
        "critical": summary.critical, "high": summary.high,
        "medium": summary.medium, "low": summary.low,
        "findings": summary.findings.iter().take(50).map(|f| serde_json::json!({
            "ruleId": f.rule_id, "severity": format!("{:?}", f.severity),
            "file": f.file_path, "line": f.line_number, "message": f.message,
            "owasp": f.owasp,
        })).collect::<Vec<_>>(),
    }))
}

// 25. codegraph_check_cwe
pub fn handle_check_cwe(project_root: &Path, directory: Option<String>) -> String {
    let dir = match directory {
        Some(ref d) => match crate::observability::validate_path(d, project_root) {
            Ok(p) => p,
            Err(e) => return json_text(&serde_json::json!({"error": e})),
        },
        None => project_root.to_path_buf(),
    };
    let summary = security::scanner::check_cwe_top25(&dir);
    json_text(&serde_json::json!({
        "standard": "CWE Top 25",
        "totalFindings": summary.total_findings,
        "critical": summary.critical, "high": summary.high,
        "medium": summary.medium, "low": summary.low,
        "findings": summary.findings.iter().take(50).map(|f| serde_json::json!({
            "ruleId": f.rule_id, "severity": format!("{:?}", f.severity),
            "file": f.file_path, "line": f.line_number, "message": f.message,
            "cwe": f.cwe,
        })).collect::<Vec<_>>(),
    }))
}

// 26. codegraph_explain_vulnerability
pub fn handle_explain_vulnerability(cwe_id: &str) -> String {
    match security::scanner::explain_vulnerability(cwe_id) {
        Some(explanation) => json_text(&serde_json::json!({
            "cweId": explanation.cwe_id, "name": explanation.name,
            "severity": explanation.severity, "description": explanation.description,
            "impact": explanation.impact, "remediation": explanation.remediation,
            "references": explanation.references,
        })),
        None => json_text(&serde_json::json!({
            "error": format!("No explanation found for {}", cwe_id),
        })),
    }
}

// 27. codegraph_suggest_fix
pub fn handle_suggest_fix(rule_id: &str, matched_code: &str) -> String {
    let finding = security::scanner::SecurityFinding {
        rule_id: rule_id.to_string(),
        rule_name: rule_id.to_string(),
        severity: security::rules::Severity::High,
        file_path: String::new(),
        line_number: 0,
        column: 0,
        matched_text: matched_code.to_string(),
        message: String::new(),
        fix: None,
        cwe: None,
        owasp: None,
        category: security::rules::RuleCategory::Other,
    };
    let fix = security::scanner::suggest_fix(&finding);
    json_text(&serde_json::json!({
        "ruleId": rule_id,
        "matchedCode": matched_code,
        "suggestedFix": fix,
    }))
}

// 28. codegraph_find_injections
pub fn handle_find_injections(source: &str, language: &str) -> String {
    let flows = security::taint::find_injection_vulnerabilities(source, language);
    json_text(&serde_json::json!({
        "vulnerabilityCount": flows.len(),
        "flows": flows.iter().map(|f| serde_json::json!({
            "type": f.vulnerability_type,
            "source": { "kind": format!("{:?}", f.source.kind), "line": f.source.line_number, "expression": f.source.expression },
            "sink": { "kind": format!("{:?}", f.sink.kind), "line": f.sink.line_number, "expression": f.sink.expression },
            "pathLength": f.path.len(),
        })).collect::<Vec<_>>(),
    }))
}

// 29. codegraph_taint_sources
pub fn handle_taint_sources(source: &str, language: &str) -> String {
    let sources = security::taint::find_taint_sources(source, language);
    json_text(&serde_json::json!({
        "sourceCount": sources.len(),
        "sources": sources.iter().map(|s| serde_json::json!({
            "kind": format!("{:?}", s.kind),
            "file": s.file_path, "line": s.line_number,
            "expression": s.expression,
        })).collect::<Vec<_>>(),
    }))
}

// 30. codegraph_security_summary
pub fn handle_security_summary(project_root: &Path, directory: Option<String>) -> String {
    let dir = match directory {
        Some(ref d) => match crate::observability::validate_path(d, project_root) {
            Ok(p) => p,
            Err(e) => return json_text(&serde_json::json!({"error": e})),
        },
        None => project_root.to_path_buf(),
    };
    let rules = security::rules::load_bundled_rules();
    let summary = security::scanner::scan_directory(&dir, &rules, true);
    json_text(&serde_json::json!({
        "riskLevel": if summary.critical > 0 { "CRITICAL" } else if summary.high > 0 { "HIGH" } else if summary.medium > 0 { "MEDIUM" } else { "LOW" },
        "totalFindings": summary.total_findings,
        "bySeverity": { "critical": summary.critical, "high": summary.high, "medium": summary.medium, "low": summary.low },
        "filesScanned": summary.files_scanned,
        "rulesApplied": summary.rules_applied,
        "topIssues": summary.top_issues,
    }))
}

// 31. codegraph_trace_taint
pub fn handle_trace_taint(source: &str, language: &str, from_line: usize) -> String {
    let flows = security::taint::trace_taint(source, language, from_line);
    json_text(&serde_json::json!({
        "fromLine": from_line,
        "flowCount": flows.len(),
        "flows": flows.iter().map(|f| serde_json::json!({
            "type": f.vulnerability_type,
            "source": { "line": f.source.line_number, "expression": f.source.expression },
            "sink": { "line": f.sink.line_number, "expression": f.sink.expression },
            "steps": f.path.iter().map(|s| serde_json::json!({
                "line": s.line_number, "code": s.code, "operation": s.operation,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    }))
}
