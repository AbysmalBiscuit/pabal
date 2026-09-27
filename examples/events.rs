//! Prints the crate's known events per harness as JSON, for the drift job.

use pabal::{ClaudeCodeEvent, CodexEvent, EventKind};

fn names<E: EventKind>() -> Vec<String> {
    // `Other` iterates with an empty name.
    let mut names: Vec<String> = E::iter()
        .map(|e| e.to_string())
        .filter(|name| !name.is_empty())
        .collect();
    names.sort();
    names
}

fn main() {
    let events = serde_json::json!({
        "claude-code": names::<ClaudeCodeEvent>(),
        "codex": names::<CodexEvent>(),
    });
    println!("{events}");
}
