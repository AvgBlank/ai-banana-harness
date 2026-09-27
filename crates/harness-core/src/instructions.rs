//! Builds the system prompt: nested XML sections, with empty sections dropped.
//!
//! ```text
//! <system-prompt>
//!   <agent-identity>...</agent-identity>
//!   <agent-capabilities>
//!     <internal-messages>...</internal-messages>
//!     <workspace>...</workspace>
//!     <deferred-tools-instructions>...</deferred-tools-instructions>   (only with deferred MCP servers)
//!     <skills>...</skills>
//!     <sub-agents>...</sub-agents>        (only when subagents are enabled)
//!   </agent-capabilities>
//!   <user-instructions><![CDATA[...]]></user-instructions>   (root thread only)
//! </system-prompt>
//! ```
//!
//! Ported from TrueForge's `InstructionBuilder`/`AgentThread.buildInstruction`, adapted to a
//! fixed shape instead of a general-purpose tree builder, since this crate only ever renders
//! this one layout.

use std::path::Path;

use crate::APP_NAME;
use crate::skills::Skill;

const ROOT_IDENTITY_TEMPLATE: &str = "You are {APP_NAME}, an autonomous coding agent that resolves tasks in the user's workspace \
using host tools, running to completion without step-by-step approval.";

const SUB_AGENT_IDENTITY: &str = "You are a sub-agent delegated a specific task by the root agent. You have the same tools as \
the root agent. Focus on completing the delegated task and return a concise result; you cannot \
ask the user questions.";

const INTERNAL_MESSAGE_RULE: &str = "Messages wrapped in <bh-internal>...</bh-internal> tags carry \
information hidden from the user; take it into account, but never write a <bh-internal> tag \
yourself.";

const SKILLS_PREAMBLE: &str = "\
1. The skill is present in the `path` directory.
2. The SKILL.md file in that directory is the primary file; there may be other files alongside it.
3. Use `description` to judge whether the skill is relevant to the current task.
4. To use a skill, read `path/SKILL.md` first with the read-file tool.
5. Do not list the skill's directory unless SKILL.md references other files and there is reason to.";

/// A deferred server's description is cut to this many characters in the prompt.
const MAX_DEFERRED_DESCRIPTION_CHARS: usize = 200;

const SUBAGENTS_TEXT: &str = "\
The Agent can delegate an independent, well-defined subtask to another agent with the \
create_sub_agent tool. Up to five subagents run in parallel. A subagent has the Agent's tools but \
no create_sub_agent or question tool of its own, and its final answer becomes the Agent's tool \
result.";

/// Everything the builder needs to render one thread's system prompt.
pub struct SystemPromptInput<'a> {
    pub workspace: &'a Path,
    /// The layered config's `agent.instructions`, shown to the model as-is.
    pub config_instructions: &'a str,
    /// The workspace's `AGENTS.md` contents, when the file exists.
    pub agents_md: Option<&'a str>,
    pub skills: &'a [Skill],
    pub subagents_enabled: bool,
    /// MCP servers behind the deferred meta-tools: name and configured description.
    pub deferred_mcp_servers: &'a [(&'a str, Option<&'a str>)],
    /// `true` for the turn's root thread; `false` for a subagent thread.
    pub root: bool,
}

pub fn system_prompt(input: SystemPromptInput<'_>) -> String {
    let identity = if input.root {
        ROOT_IDENTITY_TEMPLATE.replace("{APP_NAME}", APP_NAME)
    } else {
        SUB_AGENT_IDENTITY.to_string()
    };

    let capabilities_body = join(&[
        section("internal-messages", INTERNAL_MESSAGE_RULE),
        section("workspace", &workspace_section(input.workspace)),
        section(
            "deferred-tools-instructions",
            &deferred_tools_section(input.deferred_mcp_servers),
        ),
        section("skills", &skills_section(input.skills)),
        if input.subagents_enabled {
            section("sub-agents", SUBAGENTS_TEXT)
        } else {
            String::new()
        },
    ]);

    let user_instructions = if input.root {
        let combined = join(&[
            input.config_instructions.to_string(),
            input.agents_md.unwrap_or_default().to_string(),
        ]);
        section_cdata("user-instructions", &combined)
    } else {
        String::new()
    };

    let root_body = join(&[
        section("agent-identity", &identity),
        section("agent-capabilities", &capabilities_body),
        user_instructions,
    ]);
    section("system-prompt", &root_body)
}

fn workspace_section(workspace: &Path) -> String {
    format!(
        "The Agent's workspace is {}. All file reads, edits and commands must stay within this \
directory; the Agent's tools cannot reach outside it.",
        workspace.display()
    )
}

fn deferred_tools_section(servers: &[(&str, Option<&str>)]) -> String {
    if servers.is_empty() {
        return String::new();
    }
    let list: Vec<String> = servers
        .iter()
        .map(|(name, description)| match description {
            Some(d) => {
                let d: String = d.chars().take(MAX_DEFERRED_DESCRIPTION_CHARS).collect();
                format!("- {name}: \"{d}\"")
            }
            None => format!("- {name}"),
        })
        .collect();
    format!(
        "The following MCP servers have deferred tool loading. Their tools are NOT pre-loaded.\n\
{}\n\nThe Agent MUST use list_tools and get_tool_info to discover tools before calling them.",
        list.join("\n")
    )
}

fn skills_section(skills: &[Skill]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut parts = vec![SKILLS_PREAMBLE.to_string()];
    for skill in skills {
        parts.push(section(
            "skill",
            &format!(
                "path: {}\nname: {}\ndescription: {}",
                skill.path.display(),
                skill.name,
                skill.description
            ),
        ));
    }
    join(&parts)
}

/// `<tag>\n{body}\n</tag>`, or `""` when `body` is empty so the caller drops it.
fn section(tag: &str, body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        return String::new();
    }
    format!("<{tag}>\n{body}\n</{tag}>")
}

/// Like [`section`], but wraps the body in CDATA for arbitrary/untrusted text.
fn section_cdata(tag: &str, body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        return String::new();
    }
    format!("<{tag}><![CDATA[\n{body}\n]]></{tag}>")
}

/// Joins non-empty parts with a blank line, dropping empty ones.
fn join(parts: &[String]) -> String {
    parts
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}
