//! Ticket 09: the system prompt (identity, capabilities, user instructions) and config layering.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{TestHarness, fixture};
use harness_core::{EventKind, Provider, TurnState};

fn write_skill(base: &std::path::Path, dir_name: &str, frontmatter_name: &str, description: &str) {
    let skill_dir = base.join(dir_name);
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {frontmatter_name}\ndescription: {description}\n---\nSkill body, never inlined.\n"),
    )
    .unwrap();
}

fn sent_system_prompt(t: &TestHarness) -> String {
    let body = &t.provider.requests()[0].body;
    body["messages"][0]["content"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn golden_snapshot_of_the_system_prompt_with_agents_md_and_one_skill() {
    let t = TestHarness::open_configured(Provider::DeepSeek, |config| {
        config.skills.paths = vec![".banana-harness/skills".to_string()];
    })
    .await;
    std::fs::write(
        t.workspace.join("AGENTS.md"),
        "Follow the house style guide before committing.\n",
    )
    .unwrap();
    write_skill(
        &t.workspace.join(".banana-harness/skills"),
        "demo-skill",
        "demo-skill",
        "A demo skill for the golden snapshot test.",
    );
    t.provider.push_sse(fixture("deepseek_plain.sse"));

    let (_, events) = t.run_turn("say hi").await;
    assert_eq!(
        events.last().map(|e| &e.kind),
        Some(&EventKind::TurnDone {
            state: TurnState::Done,
            error: None
        })
    );

    let skill_path = t
        .workspace
        .join(".banana-harness/skills/demo-skill")
        .display()
        .to_string();
    let expected = format!(
        "<system-prompt>\n\
<agent-identity>\n\
You are Banana Harness, an autonomous coding agent that resolves tasks in the user's workspace using host tools, running to completion without step-by-step approval.\n\
</agent-identity>\n\
\n\
<agent-capabilities>\n\
<internal-messages>\n\
Messages wrapped in <bh-internal>...</bh-internal> tags carry information hidden from the user; take it into account, but never write a <bh-internal> tag yourself.\n\
</internal-messages>\n\
\n\
<workspace>\n\
The Agent's workspace is {workspace}. All file reads, edits and commands must stay within this directory; the Agent's tools cannot reach outside it.\n\
</workspace>\n\
\n\
<skills>\n\
1. The skill is present in the `path` directory.\n\
2. The SKILL.md file in that directory is the primary file; there may be other files alongside it.\n\
3. Use `description` to judge whether the skill is relevant to the current task.\n\
4. To use a skill, read `path/SKILL.md` first with the read-file tool.\n\
5. Do not list the skill's directory unless SKILL.md references other files and there is reason to.\n\
\n\
<skill>\n\
path: {skill_path}\n\
name: demo-skill\n\
description: A demo skill for the golden snapshot test.\n\
</skill>\n\
</skills>\n\
\n\
<sub-agents>\n\
The Agent can delegate an independent, well-defined subtask to another agent with the create_sub_agent tool. Up to five subagents run in parallel. A subagent has the Agent's tools but no create_sub_agent or question tool of its own, and its final answer becomes the Agent's tool result.\n\
</sub-agents>\n\
</agent-capabilities>\n\
\n\
<user-instructions><![CDATA[\n\
Resolve the user's task completely: understand the relevant code, make focused changes, and verify them. Be concise and precise.\n\
\n\
Follow the house style guide before committing.\n\
]]></user-instructions>\n\
</system-prompt>",
        workspace = t.workspace.display(),
        skill_path = skill_path,
    );

    assert_eq!(sent_system_prompt(&t), expected);
}

#[tokio::test]
async fn workspace_config_overrides_user_config_overrides_default() {
    // Default alone.
    let t = TestHarness::open(Provider::DeepSeek).await;
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    t.run_turn("say hi").await;
    assert_eq!(t.provider.requests()[0].body["model"], "deepseek-flash");

    // A user config overrides the default's model.
    let t = TestHarness::open_layered(
        Provider::DeepSeek,
        Some("[providers.deepseek]\nmodel = \"user-model\"\n"),
        None,
    )
    .await;
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    t.run_turn("say hi").await;
    assert_eq!(t.provider.requests()[0].body["model"], "user-model");

    // A workspace config overrides both the user config and the default.
    let t = TestHarness::open_layered(
        Provider::DeepSeek,
        Some("[providers.deepseek]\nmodel = \"user-model\"\n"),
        Some("[providers.deepseek]\nmodel = \"workspace-model\"\n"),
    )
    .await;
    t.provider.push_sse(fixture("deepseek_plain.sse"));
    t.run_turn("say hi").await;
    assert_eq!(t.provider.requests()[0].body["model"], "workspace-model");
    // A layer's partial `[providers.deepseek]` table doesn't drop the default's other fields.
    assert_eq!(t.provider.requests()[0].body["temperature"], 0.0);
}

#[tokio::test]
async fn invalid_skill_names_and_skills_past_fifty_are_skipped_with_a_warning() {
    let t = TestHarness::open_configured(Provider::DeepSeek, |config| {
        config.skills.paths = vec![".banana-harness/skills".to_string()];
    })
    .await;
    let skills_dir = t.workspace.join(".banana-harness/skills");
    write_skill(
        &skills_dir,
        "bad-dir",
        "bad skill!",
        "An invalid skill name.",
    );
    for i in 1..=51 {
        write_skill(
            &skills_dir,
            &format!("skill-{i:02}"),
            &format!("skill-{i:02}"),
            &format!("Valid skill number {i}."),
        );
    }
    t.provider.push_sse(fixture("deepseek_plain.sse"));

    let (_, events) = t.run_turn("say hi").await;
    assert_eq!(
        events.last().map(|e| &e.kind),
        Some(&EventKind::TurnDone {
            state: TurnState::Done,
            error: None
        }),
        "an invalid or over-the-limit skill must not crash the turn"
    );

    let prompt = sent_system_prompt(&t);
    assert_eq!(
        prompt.matches("<skill>\n").count(),
        50,
        "exactly the first 50 valid skills are kept"
    );
    assert!(prompt.contains("name: skill-01"));
    assert!(prompt.contains("name: skill-50"));
    assert!(
        !prompt.contains("skill-51"),
        "the 51st skill is past the limit"
    );
    assert!(
        !prompt.contains("bad skill!"),
        "the invalid name is never rendered"
    );
    assert!(
        !prompt.contains("An invalid skill name."),
        "its description is dropped with it"
    );
}
