use serde::Serialize;
use utoipa::ToSchema;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Serialize, ToSchema)]
pub struct Diagnostic {
    pub severity: Severity,
    pub node_id: Option<String>,
    pub field: String,
    pub code: String,
    pub message: String,
}

#[derive(Default, Serialize, ToSchema)]
pub struct ValidationReport {
    /// Static readiness, not permission approval or a guarantee that a provider is online.
    pub valid: bool,
    pub diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    pub(super) fn issue(
        &mut self,
        severity: Severity,
        node: Option<&str>,
        field: &str,
        code: &str,
        message: impl Into<String>,
    ) {
        self.diagnostics.push(Diagnostic {
            severity,
            node_id: node.map(str::to_owned),
            field: field.into(),
            code: code.into(),
            message: message.into(),
        });
    }
    pub(super) fn error(
        &mut self,
        node: Option<&str>,
        field: &str,
        code: &str,
        message: impl Into<String>,
    ) {
        self.issue(Severity::Error, node, field, code, message);
    }
    pub(super) fn finish(mut self) -> Self {
        self.valid = !self
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error);
        self
    }
}

impl Diagnostic {
    pub(super) fn draft(node: Option<&str>, field: &str, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            node_id: node.map(str::to_owned),
            field: field.into(),
            code: "invalid_document".into(),
            message: message.into(),
        }
    }
}
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
