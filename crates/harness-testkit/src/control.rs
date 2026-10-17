//! The stub's `/__fake/` control API. The product never calls it; Playwright and out-of-process
//! tests use it to script replies and read what the model was sent.

use serde_json::json;

use crate::stub::{ReplySpec, Shared};

/// Answers one control request with a status and a JSON body.
pub(crate) fn handle(shared: &Shared, method: &str, path: &str, body: &[u8]) -> (u16, String) {
    let route = path.trim_start_matches("/__fake/");
    match (method, route) {
        ("GET", "health") => (200, json!({ "ok": true }).to_string()),
        ("GET", "status") => (200, json!(shared.status()).to_string()),
        ("GET", "requests") => (200, json!(shared.requests()).to_string()),
        ("POST", "reset") => {
            shared.reset();
            (200, json!({ "ok": true }).to_string())
        }
        ("POST", "replies") => match serde_json::from_slice::<Vec<ReplySpec>>(body) {
            Ok(specs) => {
                for spec in &specs {
                    if let Err(message) = shared.push_spec(spec) {
                        return (400, json!({ "error": message }).to_string());
                    }
                }
                (200, json!({ "queued": specs.len() }).to_string())
            }
            Err(e) => (400, json!({ "error": e.to_string() }).to_string()),
        },
        ("POST", gate) if gate.starts_with("release/") => {
            shared.release(&gate["release/".len()..]);
            (200, json!({ "ok": true }).to_string())
        }
        _ => (404, json!({ "error": "unknown control route" }).to_string()),
    }
}
