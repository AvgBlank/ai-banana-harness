//! Test-support code shared by the crates' `tests/` folders: a fake OpenAI-compatible provider,
//! its canned replies, and git helpers. Dev-dependency only; no product crate may depend on it.
// Test support: a failed bind or poisoned lock should abort the test that hit it.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use serde_json::Value;

mod control;
mod docker;
mod driver;
mod env;
mod scenario;
mod stub;

pub use docker::FakeDocker;
pub use driver::{Outcome, Transport, run as run_scenario};
pub use env::{KeySetup, TestEnv, TestEnvOptions};
pub use scenario::{Approval, Expect, RequestContains, Scenario, Step};
pub use stub::{
    FakeProvider, Leftover, ReplySpec, Request, SseSpec, Status, StubOptions, ToolCallSpec,
};

/// A key that tests can search for in persisted rows to prove redaction.
pub const TEST_API_KEY: &str = "sk-test-0123456789abcdef";

/// Reads `fixtures/<name>` from this crate.
pub fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Runs `git` in `workspace`, failing the test if it does.
pub fn git(workspace: &Path, args: &[&str]) {
    assert!(
        std::process::Command::new("git")
            .arg("-C")
            .arg(workspace)
            .args(args)
            .status()
            .unwrap()
            .success(),
        "git {args:?} failed in {}",
        workspace.display()
    );
}

/// Stages everything in `workspace` and commits it under a throwaway identity.
pub fn commit_all(workspace: &Path, message: &str) {
    git(workspace, &["add", "-A"]);
    git(
        workspace,
        &[
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=test",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
}

/// A DeepSeek-format SSE reply that streams `content` and stops.
pub fn sse_text(content: &str) -> String {
    sse(
        &[
            serde_json::json!({"role": "assistant", "content": ""}),
            serde_json::json!({"content": content}),
        ],
        "stop",
    )
}

/// A DeepSeek-format SSE reply asking for `(id, name, arguments)` tool calls.
pub fn sse_tool_calls(calls: &[(&str, &str, &str)]) -> String {
    let mut deltas = vec![serde_json::json!({"role": "assistant", "content": ""})];
    for (index, (id, name, arguments)) in calls.iter().enumerate() {
        deltas.push(serde_json::json!({"tool_calls": [{
            "index": index,
            "id": id,
            "type": "function",
            "function": {"name": name, "arguments": arguments},
        }]}));
    }
    sse(&deltas, "tool_calls")
}

fn sse(deltas: &[Value], finish_reason: &str) -> String {
    let chunk = |delta: &Value, finish: Option<&str>| {
        serde_json::json!({
            "id": "fake",
            "object": "chat.completion.chunk",
            "model": "deepseek-flash",
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
            "usage": finish.map(|_| serde_json::json!({
                "prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15
            })),
        })
    };
    let mut out = String::new();
    for delta in deltas {
        out.push_str(&format!("data: {}\n\n", chunk(delta, None)));
    }
    out.push_str(&format!(
        "data: {}\n\n",
        chunk(&serde_json::json!({}), Some(finish_reason))
    ));
    out.push_str("data: [DONE]\n\n");
    out
}
