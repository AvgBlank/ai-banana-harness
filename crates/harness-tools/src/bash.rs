use std::time::Duration;

use serde_json::Value;

use crate::runner::{CommandRequest, Finish};
use crate::{ToolContext, ToolResult};

pub const DESCRIPTION: &str = "Run a shell command in the workspace (via `bash -lc`) and return its combined \
stdout/stderr, prefixed with an `exit code: N` line. It is killed after `timeout_secs` (default 120, at most 600). \
Dangerous commands (sudo, git push, rm -rf /, piping downloads into a shell, ...) are blocked.";

const MAX_TIMEOUT_SECS: u64 = 600;

pub fn parameters() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "command": { "type": "string", "description": "The command to run" },
            "timeout_secs": { "type": "integer", "description": "Seconds before the command is killed (max 600)" },
        },
        "required": ["command"],
    })
}

pub fn call(ctx: &ToolContext<'_>, arguments: &Value) -> ToolResult {
    let Some(command) = arguments.get("command").and_then(Value::as_str) else {
        return ToolResult::error("bash requires a string \"command\" argument");
    };
    if let Err(result) =
        crate::map_verdict(ctx.permission.check_command(command, ctx.ask_overrides))
    {
        return result;
    }
    let timeout_secs = arguments
        .get("timeout_secs")
        .and_then(Value::as_u64)
        .unwrap_or(ctx.tools.bash_timeout_secs)
        .clamp(1, MAX_TIMEOUT_SECS);
    let request = CommandRequest {
        command,
        workspace: ctx.workspace,
        hidden_env: ctx.hidden_env,
        timeout: Duration::from_secs(timeout_secs),
        output_max_bytes: ctx.tools.output_max_bytes,
        cancel: ctx.cancel,
    };
    match ctx.runner.run(&request) {
        Ok(done) => ToolResult::ok(format!(
            "{}\n{}",
            header(done.finish, timeout_secs),
            done.output
        )),
        Err(e) => ToolResult::error(format!("running bash: {e}")),
    }
}

/// The first line of the result: how the command ended.
fn header(finish: Finish, timeout_secs: u64) -> String {
    match finish {
        Finish::Exited(code) => format!("exit code: {code}"),
        Finish::Signaled(signal) => format!("killed by signal {signal}"),
        Finish::Unknown => "exit code: unknown".to_string(),
        Finish::TimedOut => format!("timed out after {timeout_secs}s; process group killed"),
        Finish::Cancelled => "cancelled; command stopped".to_string(),
    }
}
