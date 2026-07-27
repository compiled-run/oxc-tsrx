//! The project-owned diagnostic vocabulary, and the mapping from canonical OXC messages onto it.

use oxc_linter::{FixKind, Message, PossibleFixes};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSpan {
    pub offset: u32,
    pub length: u32,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineFix {
    pub offset: u32,
    pub length: u32,
    pub replacement: String,
    pub safe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineDiagnostic {
    pub rule: Option<String>,
    pub plugin: Option<String>,
    pub code: String,
    pub severity: String,
    pub message: String,
    pub labels: Vec<EngineSpan>,
    pub fixes: Vec<EngineFix>,
}

pub(super) fn map_message(message: &Message) -> EngineDiagnostic {
    let rule = message.rule.as_ref().map(|rule| rule.rule_name.to_string());
    let plugin = message.rule.as_ref().map(|rule| rule.plugin_name.to_string());
    let labels = message
        .error
        .labels
        .iter()
        .map(|label| EngineSpan {
            offset: label.offset(),
            length: label.len(),
            message: label.label().map(ToString::to_string),
        })
        .collect();
    EngineDiagnostic {
        rule,
        plugin,
        code: message.error.code.to_string(),
        severity: format!("{:?}", message.error.severity).to_ascii_lowercase(),
        message: message.error.message.to_string(),
        labels,
        fixes: fixes(&message.fixes),
    }
}

fn fixes(possible: &PossibleFixes) -> Vec<EngineFix> {
    let list = match possible {
        PossibleFixes::None => return Vec::new(),
        PossibleFixes::Single(fix) => std::slice::from_ref(fix),
        PossibleFixes::Multiple(fixes) => fixes.as_slice(),
    };
    list.iter()
        .map(|fix| EngineFix {
            offset: fix.span.start,
            length: fix.span.size(),
            replacement: fix.content.to_string(),
            safe: FixKind::SafeFix.can_apply(fix.kind),
        })
        .collect()
}
