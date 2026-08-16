use std::borrow::Cow;

/// Identifies which Responses-style tool-result family produced an output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolResultKind {
    Function,
    Custom,
}

/// Model-visible output carried by a tool-result item.
///
/// `TextParts` represents an output composed exclusively of textual parts.
/// `Unsupported` intentionally covers mixed media, images, malformed parts,
/// binary data, or any shape the aging domain cannot prove is pure text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ToolOutput {
    Text(String),
    TextParts(Vec<String>),
    Unsupported,
}

impl ToolOutput {
    pub(crate) fn textual_value(&self) -> Option<Cow<'_, str>> {
        match self {
            Self::Text(value) => Some(Cow::Borrowed(value)),
            Self::TextParts(parts) => Some(Cow::Owned(parts.concat())),
            Self::Unsupported => None,
        }
    }
}

/// Transport-agnostic normalized history item understood by the aging domain.
///
/// The transport adapter is responsible for mapping protocol-specific request
/// items into this representation and later applying any returned replacement
/// to the exact original item. Unknown shapes must become `Other` rather than
/// being guessed into a supported variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HistoryItem {
    FunctionCall {
        call_id: Option<String>,
        name: Option<String>,
    },
    CustomToolCall {
        call_id: Option<String>,
        name: Option<String>,
    },
    ToolResult {
        kind: ToolResultKind,
        call_id: Option<String>,
        output: ToolOutput,
    },
    AssistantMessage,
    Reasoning,
    Other,
}

impl HistoryItem {
    pub(crate) fn is_model_action(&self) -> bool {
        matches!(
            self,
            Self::FunctionCall { .. }
                | Self::CustomToolCall { .. }
                | Self::AssistantMessage
                | Self::Reasoning
        )
    }

    pub(crate) fn is_tool_result(&self) -> bool {
        matches!(self, Self::ToolResult { .. })
    }

    pub(crate) fn tool_call_identity(&self) -> Option<(&str, &str)> {
        match self {
            Self::FunctionCall { call_id, name } | Self::CustomToolCall { call_id, name } => {
                Some((call_id.as_deref()?, name.as_deref()?))
            }
            _ => None,
        }
    }

    pub(crate) fn tool_result(&self) -> Option<(ToolResultKind, Option<&str>, &ToolOutput)> {
        match self {
            Self::ToolResult {
                kind,
                call_id,
                output,
            } => Some((*kind, call_id.as_deref(), output)),
            _ => None,
        }
    }
}
