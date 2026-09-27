#![allow(clippy::unwrap_used, clippy::expect_used)]

use harness_core::{Event, EventKind, ModelOutput, Provider, TurnState};
use harness_tui::app::{App, SubagentStatus};
use harness_tui::ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn rendered(app: &App) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(80, 30)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn event(kind: EventKind) -> Event {
    Event {
        id: "01arz3ndektsv4rrffq69g5fav".to_string(),
        kind,
    }
}

fn message(thread: &str, id: &str, text: &str) -> [EventKind; 2] {
    [
        EventKind::ModelMessage {
            thread_id: thread.to_string(),
            message_id: id.to_string(),
            message: None,
        },
        EventKind::ModelMessage {
            thread_id: thread.to_string(),
            message_id: id.to_string(),
            message: Some(ModelOutput {
                content: text.to_string(),
                finish_reason: "stop".to_string(),
                usage: None,
            }),
        },
    ]
}

#[test]
fn a_subagent_card_renders_under_its_parent_with_the_childs_own_work_nested() {
    let mut app = App::new_main(Provider::DeepSeek, std::env::temp_dir());
    let main = app.main_mut().unwrap();
    main.push_user_message("count the fruit".to_string());

    let mut events = Vec::new();
    events.extend(message("root", "m1", "Delegating."));
    events.push(EventKind::ToolCall {
        thread_id: "root".to_string(),
        tool_call_id: "call_a".to_string(),
        name: "create_sub_agent".to_string(),
        arguments: r#"{"name":"alpha","input":"count apples"}"#.to_string(),
    });
    events.push(EventKind::ThreadCreated {
        thread_id: "child".to_string(),
        parent: "root".to_string(),
        title: "alpha".to_string(),
    });
    events.push(EventKind::ToolCall {
        thread_id: "child".to_string(),
        tool_call_id: "call_c".to_string(),
        name: "glob".to_string(),
        arguments: r#"{"pattern":"*.txt"}"#.to_string(),
    });
    events.push(EventKind::ToolResponse {
        thread_id: "child".to_string(),
        tool_call_id: "call_c".to_string(),
        content: "apples.txt".to_string(),
        blocked: false,
    });
    events.extend(message("child", "m2", "Three apples."));
    events.push(EventKind::ThreadDone {
        thread_id: "child".to_string(),
        error: None,
    });
    events.push(EventKind::ToolResponse {
        thread_id: "root".to_string(),
        tool_call_id: "call_a".to_string(),
        content: "Three apples.".to_string(),
        blocked: false,
    });
    events.extend(message("root", "m3", "There are three apples."));
    events.push(EventKind::TurnDone {
        state: TurnState::Done,
        error: None,
    });
    for kind in events {
        main.apply_event(event(kind));
    }

    // The subagent tree lists the child with its parent and outcome.
    let tree: Vec<_> = main
        .subagents()
        .map(|card| {
            (
                card.parent.as_str(),
                card.title.as_str(),
                card.status.clone(),
            )
        })
        .collect();
    assert_eq!(tree, [("root", "alpha", SubagentStatus::Done)]);

    let view = rendered(&app);
    let row = |needle: &str| {
        view.iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} not rendered:\n{}", view.join("\n")))
    };
    let spawn = row("create_sub_agent");
    let header = row("subagent alpha");
    let child_tool = row("glob");
    // The spawning call's card also previews this text, so take the later (nested) line.
    let child_text = view
        .iter()
        .rposition(|line| line.contains("Three apples."))
        .unwrap();
    let final_answer = row("There are three apples.");
    // The card sits under the parent's spawning call, before the parent's final answer, and
    // the child's own tool call and text are indented inside it.
    assert!(
        spawn < header && header < child_tool && child_tool < child_text,
        "{}",
        view.join("\n")
    );
    assert!(child_text < final_answer);
    let nested = |row: usize| {
        view[row]
            .strip_prefix('│')
            .is_some_and(|inside| inside.trim_start().starts_with('┊'))
    };
    assert!(
        nested(child_tool) && nested(child_text),
        "{}",
        view.join("\n")
    );
    assert!(!nested(final_answer) && !nested(spawn));
}
