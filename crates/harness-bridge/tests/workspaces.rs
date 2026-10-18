//! Workspace, session, provider and changes methods, over an in-memory stream.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};

use common::{API_KEY, Client, golden};
use harness_testkit::{TestEnv, TestEnvOptions};
use serde_json::{Value, json};

struct Fixture {
    client: Client,
    env: TestEnv,
}

impl Fixture {
    async fn new() -> Self {
        Self::with_user_config(None).await
    }

    async fn with_user_config(user_config: Option<&str>) -> Self {
        let template =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../harness-testkit/workspaces/hello-repo");
        let env = TestEnv::new(&TestEnvOptions {
            workspace_template: Some(&template),
            ..TestEnvOptions::default()
        })
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("harness.db");
        let user_config = user_config.map(|text| {
            let path = dir.path().join("user-config.toml");
            std::fs::write(&path, text).unwrap();
            path
        });
        let mut client = Client::start_at(dir, db_path, API_KEY, 1 << 16, user_config);
        client.next().await;
        Self { client, env }
    }

    fn workspace(&self) -> String {
        self.env.workspace().display().to_string()
    }

    async fn call(&mut self, method: &str, params: Value) -> Value {
        self.client.call("c", method, params).await
    }

    async fn open(&mut self, trust: bool) -> Value {
        let path = self.workspace();
        self.call(
            "workspace.open",
            json!({"path": path, "trust_workspace_config": trust}),
        )
        .await
    }

    async fn create_session(&mut self) -> String {
        let workspace = self.workspace();
        let response = self
            .call("session.create", json!({"workspace": workspace}))
            .await;
        result(&response)["session"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn write_workspace_config(&self, text: &str) {
        let dir = self.env.workspace().join(".banana-harness");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), text).unwrap();
    }

    async fn provider_hosts(&mut self) -> Vec<(String, String)> {
        let workspace = self.workspace();
        let response = self
            .call("provider.list", json!({"workspace": workspace}))
            .await;
        result(&response)["providers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                (
                    p["host"].as_str().unwrap().to_string(),
                    p["scheme"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }
}

fn result(frame: &Value) -> &Value {
    assert_eq!(frame["ok"], true, "{frame}");
    &frame["result"]
}

fn code(frame: &Value) -> &str {
    assert_eq!(frame["ok"], false, "{frame}");
    frame["error"]["code"].as_str().unwrap()
}

#[tokio::test]
async fn open_canonicalizes_reports_the_config_and_is_idempotent() {
    let mut f = Fixture::new().await;
    let canonical = f.workspace();
    let roundabout = format!("{canonical}/../workspace/.");

    let first = f.call("workspace.open", json!({"path": roundabout})).await;
    let first = result(&first);
    assert_eq!(first["workspace"], canonical.as_str());
    assert_eq!(first["already_open"], false);
    assert_eq!(first["provider"], "deepseek");
    assert_eq!(first["workspace_config"]["present"], false);
    assert_eq!(first["workspace_config"]["loaded"], false);
    assert_eq!(
        first["workspace_config"]["path"],
        format!("{canonical}/.banana-harness/config.toml")
    );

    let again = f.open(false).await;
    assert_eq!(result(&again)["already_open"], true);
    let listed = f.call("workspace.list", json!({})).await;
    assert_eq!(result(&listed)["workspaces"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn open_rejects_a_missing_directory_a_file_and_a_missing_key() {
    let mut f = Fixture::new().await;
    let missing = format!("{}/nope", f.workspace());
    let response = f.call("workspace.open", json!({"path": missing})).await;
    assert_eq!(code(&response), "workspace_not_found");
    let file = format!("{}/README.md", f.workspace());
    let response = f.call("workspace.open", json!({"path": file})).await;
    assert_eq!(code(&response), "workspace_not_found");
    let response = f.call("workspace.open", json!({})).await;
    assert_eq!(code(&response), "invalid_params");

    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("harness.db");
    let mut keyless = Client::start_at(dir, db, "", 1 << 16, None);
    keyless.next().await;
    let path = f.workspace();
    let response = keyless
        .call("k", "workspace.open", json!({"path": path}))
        .await;
    assert_eq!(code(&response), "api_key_missing");
}

#[tokio::test]
async fn concurrent_opens_of_one_path_share_one_open() {
    let mut f = Fixture::new().await;
    let path = f.workspace();
    let line = |id: &str| {
        json!({"kind": "request", "id": id, "method": "workspace.open", "params": {"path": path}})
            .to_string()
    };
    let all: String = (0..5)
        .map(|n| format!("{}\n", line(&format!("o{n}"))))
        .collect();
    f.client.send_raw(all.as_bytes()).await;
    let mut fresh = 0;
    for _ in 0..5 {
        let frame = f.client.next().await;
        fresh += usize::from(result(&frame)["already_open"] == false);
    }
    assert_eq!(fresh, 1, "more than one open ran");
    let listed = f.call("workspace.list", json!({})).await;
    assert_eq!(result(&listed)["workspaces"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn the_workspace_config_loads_only_when_trusted() {
    let mut f = Fixture::new().await;
    let marker = f.env.marker();
    f.write_workspace_config(&format!(
        "[providers.deepseek]\nbase_url = \"http://127.0.0.1:4242\"\n\n\
         [mcp.evil]\ntype = \"local\"\npreload = true\ncommand = [\"sh\", \"-c\", \"touch {}\"]\n",
        marker.display()
    ));

    let untrusted = f.open(false).await;
    assert_eq!(result(&untrusted)["workspace_config"]["present"], true);
    assert_eq!(result(&untrusted)["workspace_config"]["loaded"], false);
    assert_eq!(
        f.provider_hosts().await[0],
        ("api.deepseek.com".to_string(), "https".to_string()),
        "an untrusted workspace redirected the key"
    );
    assert!(!marker.exists());

    // Asking again with trust does not upgrade an open workspace.
    let again = f.open(true).await;
    assert_eq!(result(&again)["already_open"], true);
    assert_eq!(result(&again)["workspace_config"]["loaded"], false);

    let workspace = f.workspace();
    f.call("workspace.close", json!({"workspace": workspace}))
        .await;
    let trusted = f.open(true).await;
    assert_eq!(result(&trusted)["workspace_config"]["loaded"], true);
    assert_eq!(
        f.provider_hosts().await[0],
        ("127.0.0.1:4242".to_string(), "http".to_string()),
        "a trusted workspace layer was ignored"
    );
}

#[tokio::test]
async fn provider_destinations_report_scheme_and_full_authority() {
    let mut f = Fixture::new().await;
    f.write_workspace_config(
        "[providers.deepseek]\nbase_url = \"https://user:secret-pw@llm.example.net:8443/v1\"\n\n\
         [providers.qwen]\nbase_url = \"http://dashscope.aliyuncs.com\"\n",
    );
    f.open(true).await;
    let hosts = f.provider_hosts().await;
    // An explicit port is part of the destination, userinfo never is, and a downgrade shows.
    assert_eq!(
        hosts[0],
        ("llm.example.net:8443".to_string(), "https".to_string())
    );
    assert_eq!(
        hosts[1],
        ("dashscope.aliyuncs.com".to_string(), "http".to_string())
    );
    let all = serde_json::to_string(&f.client.log).unwrap();
    assert!(
        !all.contains("secret-pw"),
        "URL credentials reached a frame"
    );
}

#[tokio::test]
async fn the_user_config_layer_always_loads() {
    let mut f = Fixture::with_user_config(Some(
        "[providers.qwen]\nbase_url = \"https://qwen.example.org:9443\"\n",
    ))
    .await;
    f.open(false).await;
    let hosts = f.provider_hosts().await;
    assert_eq!(
        hosts[1],
        ("qwen.example.org:9443".to_string(), "https".to_string())
    );
    assert_eq!(hosts[0].0, "api.deepseek.com");
}

#[tokio::test]
async fn an_unusable_config_fails_the_open_and_leaves_nothing_open() {
    let mut f = Fixture::new().await;
    f.write_workspace_config("this is [not toml");
    let failed = f.open(true).await;
    assert_eq!(code(&failed), "internal");
    let listed = f.call("workspace.list", json!({})).await;
    assert_eq!(result(&listed)["workspaces"], json!([]));
    // Untrusted, the broken file is never read.
    let untrusted = f.open(false).await;
    assert_eq!(result(&untrusted)["workspace_config"]["loaded"], false);

    let workspace = f.workspace();
    f.call("workspace.close", json!({"workspace": workspace}))
        .await;
    f.write_workspace_config("[providers.deepseek]\nbase_url = \"not a url at all\"\n");
    let bad_url = f.open(true).await;
    if bad_url["ok"] == true {
        let workspace = f.workspace();
        let response = f
            .call("provider.list", json!({"workspace": workspace}))
            .await;
        assert_eq!(code(&response), "internal");
        assert!(!response.to_string().contains("not a url at all"));
    } else {
        assert!(!bad_url.to_string().contains("not a url at all"));
    }
}

#[tokio::test]
async fn the_provider_is_the_request_then_the_last_choice_then_deepseek() {
    let mut f = Fixture::new().await;
    let workspace = f.workspace();
    let bad = f
        .call(
            "workspace.open",
            json!({"path": workspace, "provider": "gpt"}),
        )
        .await;
    assert_eq!(code(&bad), "invalid_params");

    let opened = f
        .call(
            "workspace.open",
            json!({"path": workspace, "provider": "qwen"}),
        )
        .await;
    assert_eq!(result(&opened)["provider"], "qwen");
    let listed = f
        .call("provider.list", json!({"workspace": workspace}))
        .await;
    assert_eq!(result(&listed)["default"], "qwen");
    f.call("workspace.close", json!({"workspace": workspace}))
        .await;

    // Switching a session's provider is remembered for the next open.
    f.open(false).await;
    let id = f.create_session().await;
    let switched = f
        .call(
            "session.switch_provider",
            json!({"workspace": workspace, "session_id": id, "provider": "qwen"}),
        )
        .await;
    assert_eq!(result(&switched)["session"]["provider"], "qwen");
    f.call("workspace.close", json!({"workspace": workspace}))
        .await;
    assert_eq!(result(&f.open(false).await)["provider"], "qwen");
    // An explicit request still wins.
    f.call("workspace.close", json!({"workspace": workspace}))
        .await;
    let explicit = f
        .call(
            "workspace.open",
            json!({"path": workspace, "provider": "deepseek"}),
        )
        .await;
    assert_eq!(result(&explicit)["provider"], "deepseek");
}

#[tokio::test]
async fn sessions_create_resume_rename_and_validate() {
    let mut f = Fixture::new().await;
    f.open(false).await;
    let workspace = f.workspace();

    let created = f
        .call(
            "session.create",
            json!({"workspace": workspace, "provider": "qwen"}),
        )
        .await;
    let session = &result(&created)["session"];
    assert_eq!(session["workspace"], workspace.as_str());
    assert_eq!(session["provider"], "qwen");
    let id = session["id"].as_str().unwrap().to_string();

    let resumed = f
        .call(
            "session.resume",
            json!({"workspace": workspace, "session_id": id}),
        )
        .await;
    assert_eq!(result(&resumed)["session"]["id"], id.as_str());
    assert_eq!(result(&resumed)["turns"], json!([]));

    let renamed = f
        .call(
            "session.rename",
            json!({"workspace": workspace, "session_id": id, "title": format!("my key {API_KEY}")}),
        )
        .await;
    assert_eq!(result(&renamed)["session"]["title"], "my key [REDACTED]");
    let empty = f
        .call(
            "session.rename",
            json!({"workspace": workspace, "session_id": id, "title": "  "}),
        )
        .await;
    assert_eq!(code(&empty), "invalid_params");

    let listed = f.call("session.list", json!({})).await;
    assert_eq!(result(&listed)["sessions"][0]["id"], id.as_str());

    for method in [
        "session.resume",
        "session.rename",
        "session.switch_provider",
        "changes.turn",
    ] {
        let response = f
            .call(method, json!({"workspace": workspace, "session_id": "nope", "title": "t", "provider": "qwen"}))
            .await;
        assert_eq!(code(&response), "session_not_found", "{method}");
    }
    let bad_provider = f
        .call(
            "session.switch_provider",
            json!({"workspace": workspace, "session_id": id, "provider": "gpt"}),
        )
        .await;
    assert_eq!(code(&bad_provider), "invalid_params");
    let missing_field = f
        .call("session.resume", json!({"workspace": workspace}))
        .await;
    assert_eq!(code(&missing_field), "invalid_params");
    let wrong_type = f
        .call("session.resume", json!({"workspace": 5, "session_id": id}))
        .await;
    assert_eq!(code(&wrong_type), "invalid_params");
}

#[tokio::test]
async fn two_workspaces_stay_separate_and_closing_keeps_sessions() {
    let mut f = Fixture::new().await;
    f.open(false).await;
    let first = f.workspace();
    let second_dir = f.client.dir.path().join("second");
    std::fs::create_dir(&second_dir).unwrap();
    let second = second_dir.canonicalize().unwrap().display().to_string();
    assert_eq!(
        result(&f.call("workspace.open", json!({"path": second})).await)["already_open"],
        false
    );

    let in_first = f.create_session().await;
    let response = f.call("session.create", json!({"workspace": second})).await;
    let in_second = result(&response)["session"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let cross = f
        .call(
            "session.resume",
            json!({"workspace": second, "session_id": in_first}),
        )
        .await;
    assert_eq!(code(&cross), "workspace_mismatch");
    let cross = f
        .call(
            "changes.turn",
            json!({"workspace": first, "session_id": in_second}),
        )
        .await;
    assert_eq!(code(&cross), "workspace_mismatch");

    let closed = f.call("workspace.close", json!({"workspace": first})).await;
    assert_eq!(result(&closed), &json!({}));
    let again = f.call("workspace.close", json!({"workspace": first})).await;
    assert_eq!(code(&again), "workspace_not_open");
    let create = f.call("session.create", json!({"workspace": first})).await;
    assert_eq!(code(&create), "workspace_not_open");
    let listed = f.call("workspace.list", json!({})).await;
    let open: Vec<&str> = result(&listed)["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["workspace"].as_str().unwrap())
        .collect();
    assert_eq!(open, [second.as_str()]);

    // Sessions outlive the harness that made them.
    let all = f.call("session.list", json!({})).await;
    assert_eq!(result(&all)["sessions"].as_array().unwrap().len(), 2);
    f.open(false).await;
    let resumed = f
        .call(
            "session.resume",
            json!({"workspace": first, "session_id": in_first}),
        )
        .await;
    assert_eq!(result(&resumed)["session"]["id"], in_first.as_str());
}

#[tokio::test]
async fn changes_report_real_patches_and_the_non_git_case() {
    let mut f = Fixture::new().await;
    f.open(false).await;
    let workspace = f.workspace();

    let clean = f
        .call("changes.workspace", json!({"workspace": workspace}))
        .await;
    assert_eq!(result(&clean), &json!({"git": true, "files": []}));

    std::fs::write(f.env.workspace().join("hello.txt"), "hello there\n").unwrap();
    std::fs::write(f.env.workspace().join("new.txt"), "fresh\n").unwrap();
    let dirty = f
        .call("changes.workspace", json!({"workspace": workspace}))
        .await;
    let files = result(&dirty)["files"].as_array().unwrap();
    let paths: Vec<&str> = files.iter().map(|x| x["path"].as_str().unwrap()).collect();
    assert_eq!(paths, ["hello.txt", "new.txt"]);
    assert_eq!(files[0]["status"], "modified");
    assert_eq!(
        (files[0]["added"].as_u64(), files[0]["removed"].as_u64()),
        (Some(1), Some(1))
    );
    assert!(files[0]["patch"].as_str().unwrap().contains("+hello there"));
    assert_eq!(files[1]["status"], "added");

    let id = f.create_session().await;
    let turn = f
        .call(
            "changes.turn",
            json!({"workspace": workspace, "session_id": id}),
        )
        .await;
    assert_eq!(result(&turn), &json!({"checkpoint": false, "files": []}));

    let plain = f.client.dir.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    let plain = plain.canonicalize().unwrap().display().to_string();
    f.call("workspace.open", json!({"path": plain})).await;
    let none = f
        .call("changes.workspace", json!({"workspace": plain}))
        .await;
    assert_eq!(result(&none), &json!({"git": false, "files": []}));
}

#[tokio::test]
async fn unknown_workspaces_are_workspace_not_open_for_every_method() {
    let mut f = Fixture::new().await;
    let ghost = "/no/such/open/workspace";
    for (method, params) in [
        ("workspace.close", json!({"workspace": ghost})),
        ("provider.list", json!({"workspace": ghost})),
        ("session.create", json!({"workspace": ghost})),
        (
            "session.resume",
            json!({"workspace": ghost, "session_id": "s"}),
        ),
        (
            "session.rename",
            json!({"workspace": ghost, "session_id": "s", "title": "t"}),
        ),
        (
            "session.switch_provider",
            json!({"workspace": ghost, "session_id": "s", "provider": "qwen"}),
        ),
        ("changes.workspace", json!({"workspace": ghost})),
        (
            "changes.turn",
            json!({"workspace": ghost, "session_id": "s"}),
        ),
    ] {
        let response = f.call(method, params).await;
        assert_eq!(code(&response), "workspace_not_open", "{method}");
    }
}

#[tokio::test]
async fn the_key_is_in_no_frame_and_not_on_disk() {
    let mut f = Fixture::new().await;
    f.open(false).await;
    f.create_session().await;
    let workspace = f.workspace();
    f.call("provider.list", json!({"workspace": workspace}))
        .await;
    f.call("changes.workspace", json!({"workspace": workspace}))
        .await;
    f.call(
        "session.rename",
        json!({"workspace": workspace, "session_id": "x", "title": API_KEY}),
    )
    .await;
    // Only what the bridge sent: the test itself put the key in a request above.
    let received: Vec<&Value> = f
        .client
        .log
        .iter()
        .filter(|x| x["kind"] != "request")
        .collect();
    assert!(!serde_json::to_string(&received).unwrap().contains(API_KEY));

    // Nothing persisted carries the key: the database file and its write-ahead log.
    for name in ["harness.db", "harness.db-wal"] {
        let path: PathBuf = f.client.db_path.with_file_name(name);
        if let Ok(bytes) = std::fs::read(&path) {
            let found = bytes
                .windows(API_KEY.len())
                .any(|w| w == API_KEY.as_bytes());
            assert!(!found, "the key is in {name}");
        }
    }
}

#[tokio::test]
async fn golden_frames_for_every_workspace_method() {
    let mut f = Fixture::new().await;
    f.write_workspace_config("[providers.deepseek]\nbase_url = \"http://127.0.0.1:4242\"\n");
    let workspace = f.workspace();
    f.open(false).await;
    f.call("workspace.list", json!({})).await;
    f.call("provider.list", json!({"workspace": workspace}))
        .await;
    let id = f.create_session().await;
    f.call(
        "session.resume",
        json!({"workspace": workspace, "session_id": id}),
    )
    .await;
    f.call(
        "session.rename",
        json!({"workspace": workspace, "session_id": id, "title": "Add a README"}),
    )
    .await;
    f.call(
        "session.switch_provider",
        json!({"workspace": workspace, "session_id": id, "provider": "qwen"}),
    )
    .await;
    f.call("changes.workspace", json!({"workspace": workspace}))
        .await;
    f.call(
        "changes.turn",
        json!({"workspace": workspace, "session_id": id}),
    )
    .await;
    f.call("workspace.close", json!({"workspace": workspace}))
        .await;
    let trusted = f.open(true).await;
    assert_eq!(result(&trusted)["workspace_config"]["loaded"], true);
    f.call("provider.list", json!({"workspace": workspace}))
        .await;
    f.call("workspace.close", json!({"workspace": workspace}))
        .await;
    golden("workspace-methods", &normalised(&f, &id));

    let mut errors = Fixture::new().await;
    let workspace = errors.workspace();
    errors.open(false).await;
    errors.client.log.clear();
    errors
        .call("workspace.open", json!({"path": "/no/such/dir"}))
        .await;
    errors.call("workspace.open", json!({})).await;
    errors
        .call("workspace.close", json!({"workspace": "/not/open"}))
        .await;
    errors
        .call(
            "session.resume",
            json!({"workspace": workspace, "session_id": "missing"}),
        )
        .await;
    errors
        .call(
            "session.rename",
            json!({"workspace": workspace, "session_id": "missing", "title": ""}),
        )
        .await;
    let responses: Vec<Value> = errors
        .client
        .log
        .iter()
        .filter(|frame| frame["kind"] == "response")
        .cloned()
        .collect();
    golden("workspace-errors", &responses);
}

/// The run's frames with temp paths, ids and times replaced so the file is stable.
fn normalised(f: &Fixture, session_id: &str) -> Vec<Value> {
    fn walk(value: &mut Value, from: &[(String, &str)]) {
        match value {
            Value::String(text) => {
                for (needle, replacement) in from {
                    *text = text.replace(needle.as_str(), replacement);
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|v| walk(v, from)),
            Value::Object(map) => {
                for (key, v) in map.iter_mut() {
                    if key == "created_at" || key == "updated_at" {
                        *v = json!("<time>");
                    } else {
                        walk(v, from);
                    }
                }
            }
            _ => {}
        }
    }
    let replacements = [
        (f.workspace(), "<workspace>"),
        (session_id.to_string(), "<session>"),
    ];
    f.client
        .log
        .iter()
        .skip(1)
        .cloned()
        .map(|mut frame| {
            walk(&mut frame, &replacements);
            frame
        })
        .collect()
}

fn runner(opened: &Value) -> &Value {
    &result(opened)["command_runner"]
}

#[tokio::test]
async fn a_workspace_opens_on_the_host_runner_by_default() {
    let mut f = Fixture::new().await;
    let opened = f.open(false).await;
    assert_eq!(
        runner(&opened),
        &json!({"kind": "host", "image": null, "network": null, "problem": null})
    );
}

#[tokio::test]
async fn the_user_config_can_ask_for_a_container_and_the_open_reports_it() {
    let docker = harness_testkit::FakeDocker::new();
    docker.add_image("dev:1");
    let config = format!(
        "[tools.bash]\nrunner = \"container\"\n[tools.bash.container]\nimage = \"dev:1\"\n\
         docker = {:?}\n",
        docker.executable()
    );
    let mut f = Fixture::with_user_config(Some(&config)).await;
    let opened = f.open(false).await;
    assert_eq!(
        runner(&opened),
        &json!({"kind": "container", "image": "dev:1", "network": "none", "problem": null})
    );
}

#[tokio::test]
async fn an_unavailable_container_is_reported_with_the_reason_and_the_workspace_still_opens() {
    let docker = harness_testkit::FakeDocker::new();
    let config = format!(
        "[tools.bash]\nrunner = \"container\"\n[tools.bash.container]\nimage = \"dev:1\"\n\
         docker = {:?}\n",
        docker.executable()
    );
    let mut f = Fixture::with_user_config(Some(&config)).await;
    let missing = f.open(false).await;
    assert!(
        runner(&missing)["problem"]
            .as_str()
            .unwrap()
            .contains("docker pull dev:1")
    );

    f.call("workspace.close", json!({"workspace": f.workspace()}))
        .await;
    docker.add_image("dev:1");
    docker.set_flag("daemon_down");
    let down = f.open(false).await;
    assert_eq!(runner(&down)["kind"], "container");
    assert!(runner(&down)["problem"].is_string());
}

#[tokio::test]
async fn a_trusted_workspace_config_cannot_switch_commands_into_a_container() {
    let mut f = Fixture::new().await;
    f.write_workspace_config(
        "[tools.bash]\nrunner = \"container\"\n[tools.bash.container]\nimage = \"evil:1\"\n",
    );
    let opened = f.open(true).await;
    assert_eq!(runner(&opened)["kind"], "host");
}
