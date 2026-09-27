//! Tools the harness answers itself rather than handing to `harness-tools`.

use harness_llm::ToolDefinition;
use serde::Deserialize;
use serde_json::json;

use crate::config::AgentConfig;

/// How many subagent threads may run at the same time.
pub(crate) const MAX_PARALLEL_SUB_AGENTS: usize = 5;

/// A question's answer options; more than this is a tool-level error.
pub(crate) const MAX_QUESTION_OPTIONS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Builtin {
    CreateSubAgent,
    AskQuestion,
}

impl Builtin {
    const ALL: [Builtin; 2] = [Builtin::CreateSubAgent, Builtin::AskQuestion];

    fn name(self) -> &'static str {
        match self {
            Builtin::CreateSubAgent => "create_sub_agent",
            Builtin::AskQuestion => "ask_question",
        }
    }

    /// Root-only tools are never offered to, or callable by, a subagent thread.
    fn root_only(self) -> bool {
        match self {
            Builtin::CreateSubAgent | Builtin::AskQuestion => true,
        }
    }

    fn enabled(self, config: &AgentConfig) -> bool {
        match self {
            Builtin::CreateSubAgent => config.subagents,
            Builtin::AskQuestion => true,
        }
    }

    fn available(self, root: bool, config: &AgentConfig) -> bool {
        (root || !self.root_only()) && self.enabled(config)
    }

    /// The builtins a thread may use, as wire definitions.
    pub(crate) fn definitions(root: bool, config: &AgentConfig) -> Vec<ToolDefinition> {
        Self::ALL
            .into_iter()
            .filter(|b| b.available(root, config))
            .map(Builtin::definition)
            .collect()
    }

    /// The builtin a thread's call names, if that thread may use it; anything else falls
    /// through to the host tools (which report an unknown name as an error).
    pub(crate) fn find(name: &str, root: bool, config: &AgentConfig) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|b| b.name() == name && b.available(root, config))
    }

    fn definition(self) -> ToolDefinition {
        match self {
            Builtin::CreateSubAgent => ToolDefinition {
                name: self.name().to_string(),
                description: CREATE_SUB_AGENT_DESCRIPTION.to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "name": {"type": "string", "description": "A short name for the subagent."},
                        "input": {"type": "string", "description": "The self-contained task for the subagent."},
                    },
                    "required": ["name", "input"],
                }),
            },
            Builtin::AskQuestion => ToolDefinition {
                name: self.name().to_string(),
                description: ASK_QUESTION_DESCRIPTION.to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "question": {"type": "string", "description": "The question to ask the user."},
                        "options": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Up to five suggested answers; the user may also answer freely.",
                        },
                    },
                    "required": ["question"],
                }),
            },
        }
    }
}

// Ported from TrueForge's `DynamicSubAgents`, minus the sandbox and model-set wording.
const CREATE_SUB_AGENT_DESCRIPTION: &str = "\
Delegate a well-defined task to a sub-agent. The Agent must not delegate the whole work to the \
sub-agent.

The sub-agent has access to the same tools and the same workspace as the parent Agent. Files \
created or modified by either agent are shared and remain available to both.

The sub-agent has NO access to the prior conversation or the user's original message. The Agent \
must provide a clear, self-contained instruction that includes all necessary context, the exact \
task to perform, any constraints, what outputs are expected, and what work has already been \
completed so effort is not duplicated.";

const ASK_QUESTION_DESCRIPTION: &str = "\
Ask the user a question to resolve genuine ambiguity, with up to five suggested options; the \
user may also answer with free text. The turn waits in place for the answer.";

/// `create_sub_agent`'s arguments.
#[derive(Debug, Deserialize)]
pub(crate) struct SubAgentRequest {
    pub name: String,
    pub input: String,
}

impl SubAgentRequest {
    pub(crate) fn parse(arguments: &str) -> Result<Self, String> {
        let request: Self = serde_json::from_str(arguments)
            .map_err(|e| format!("invalid create_sub_agent arguments: {e}"))?;
        if request.name.trim().is_empty() || request.input.trim().is_empty() {
            return Err("create_sub_agent needs a non-empty name and input".to_string());
        }
        Ok(request)
    }
}

/// `ask_question`'s arguments.
#[derive(Debug, Deserialize)]
pub(crate) struct QuestionRequest {
    pub question: String,
    #[serde(default)]
    pub options: Vec<String>,
}

impl QuestionRequest {
    pub(crate) fn parse(arguments: &str) -> Result<Self, String> {
        let request: Self = serde_json::from_str(arguments)
            .map_err(|e| format!("invalid ask_question arguments: {e}"))?;
        if request.question.trim().is_empty() {
            return Err("ask_question needs a non-empty question".to_string());
        }
        if request.options.len() > MAX_QUESTION_OPTIONS {
            return Err(format!(
                "ask_question allows at most {MAX_QUESTION_OPTIONS} options"
            ));
        }
        Ok(request)
    }
}
