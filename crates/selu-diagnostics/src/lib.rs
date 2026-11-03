//! Diagnostics with stable codes and readable terminal rendering.
//!
//! Every user-facing error in Selu is a [`Diagnostic`]: a stable code
//! (`SELU0001`...), a primary span, a plain message, and optionally a help
//! note and a one-line fix suggestion.

use codespan_reporting::diagnostic::{Diagnostic as CsDiagnostic, Label, Severity as CsSeverity};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, termcolor::NoColor};
use serde::{Deserialize, Serialize};
use std::ops::Range;

/// A byte span in a named source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub file: String,
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(file: impl Into<String>, start: usize, end: usize) -> Self {
        Self {
            file: file.into(),
            start,
            end,
        }
    }
    pub fn range(&self) -> Range<usize> {
        self.start..self.end.max(self.start)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
}

/// The fields of a [`Diagnostic`], boxed inside it so `Result<T, Diagnostic>`
/// stays small on the happy path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticData {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub span: Option<Span>,
    pub span_label: Option<String>,
    pub help: Option<String>,
    /// A concrete fix suggestion, e.g. "add `;` here".
    pub fix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Diagnostic(Box<DiagnosticData>);

impl std::ops::Deref for Diagnostic {
    type Target = DiagnosticData;
    fn deref(&self) -> &DiagnosticData {
        &self.0
    }
}

impl Diagnostic {
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self(Box::new(DiagnosticData {
            code: code.to_string(),
            severity: Severity::Error,
            message: message.into(),
            span: None,
            span_label: None,
            help: None,
            fix: None,
        }))
    }
    pub fn warning(code: &str, message: impl Into<String>) -> Self {
        let mut d = Self::error(code, message);
        d.0.severity = Severity::Warning;
        d
    }
    pub fn with_span(mut self, span: Span) -> Self {
        self.0.span = Some(span);
        self
    }
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.0.span_label = Some(label.into());
        self
    }
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.0.help = Some(help.into());
        self
    }
    pub fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.0.fix = Some(fix.into());
        self
    }

    /// Render against `source` (the text of `span.file`) as plain text.
    pub fn render(&self, source: &str) -> String {
        let file_name = self
            .span
            .as_ref()
            .map(|s| s.file.clone())
            .unwrap_or_else(|| "<input>".to_string());
        let file = SimpleFile::new(file_name, source);
        let mut cs = CsDiagnostic::new(match self.severity {
            Severity::Error => CsSeverity::Error,
            Severity::Warning => CsSeverity::Warning,
        })
        .with_code(self.code.clone())
        .with_message(self.message.clone());
        if let Some(span) = &self.span {
            cs = cs.with_labels(vec![
                Label::primary((), span.range())
                    .with_message(self.span_label.clone().unwrap_or_default()),
            ]);
        }
        let mut notes = Vec::new();
        if let Some(help) = &self.help {
            notes.push(format!("help: {help}"));
        }
        if let Some(fix) = &self.fix {
            notes.push(format!("fix: {fix}"));
        }
        if !notes.is_empty() {
            cs = cs.with_notes(notes);
        }
        let mut writer = NoColor::new(Vec::new());
        let config = term::Config::default();
        if term::emit(&mut writer, &config, &file, &cs).is_err() {
            return format!("[{}] {}", self.code, self.message);
        }
        String::from_utf8(writer.into_inner()).unwrap_or_else(|_| self.message.clone())
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)?;
        if let Some(fix) = &self.fix {
            write!(f, " (fix: {fix})")?;
        }
        Ok(())
    }
}
impl std::error::Error for Diagnostic {}

#[derive(Debug, Default, Clone)]
pub struct DiagnosticBag(pub Vec<Diagnostic>);

impl DiagnosticBag {
    pub fn push(&mut self, d: Diagnostic) {
        self.0.push(d);
    }
    pub fn has_errors(&self) -> bool {
        self.0.iter().any(|d| d.severity == Severity::Error)
    }
    pub fn render_all(&self, source: &str) -> String {
        self.0
            .iter()
            .map(|d| d.render(source))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_code_message_and_fix() {
        let src = "const x = 1\nconst y = 2;\n";
        let d = Diagnostic::error("SELU0101", "missing `;` after this statement")
            .with_span(Span::new("a.selu", 11, 12))
            .with_fix("add `;` at the end of the line");
        let out = d.render(src);
        assert!(out.contains("SELU0101"), "{out}");
        assert!(out.contains("missing `;`"), "{out}");
        assert!(out.contains("add `;`"), "{out}");
        assert!(out.contains("a.selu"), "{out}");
    }
}
