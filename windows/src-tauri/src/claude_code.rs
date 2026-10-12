// Claude Code as a chat provider: the user's own `claude` CLI answers in print
// mode with their Claude subscription, so no API key is needed. Each turn is
// one headless run in an empty folder of its own: web search and web fetch are
// its only tools (a question can never edit a file or run a command), safe mode
// leaves out the user's hooks, plugins, MCP servers and CLAUDE.md, and nothing
// is saved as a session. It is also where a Claude turn goes when there is no
// API key or no credit left on it (chat.rs).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{mpsc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use crate::chat::{self, Chat, ChatContext, ChatReply, ModelInfo};
use crate::i18n::t;
use crate::platform;

pub const PROVIDER: &str = "claude-code";
pub const DEFAULT_MODEL: &str = "sonnet";
const TIMEOUT: Duration = Duration::from_secs(180);
/// All we are willing to read of its answer.
const MAX_OUTPUT: u64 = 4 * 1024 * 1024;
const TOOLS: &str = "WebSearch,WebFetch";

/// The run in progress, so a new chat ends it.
static RUNNING: Mutex<Option<u32>> = Mutex::new(None);

pub fn find() -> Option<PathBuf> {
    platform::cli_candidates("claude").into_iter().next()
}

fn not_installed() -> String {
    t("Claude Code isn't installed.")
}

/// Ends the run in progress, if any: the chat was reset or closed.
pub fn stop() {
    if let Some(pid) = RUNNING.lock().unwrap().take() {
        platform::kill_tree(pid);
    }
}

/// The command line after `claude`. The system prompt is read from a file:
/// a .cmd install would otherwise pass it through cmd.exe's quoting.
pub fn args(model: &str, system_file: &Path) -> Vec<String> {
    let mut out: Vec<String> = [
        "-p", "--output-format", "json", "--no-session-persistence", "--safe-mode",
        "--tools", TOOLS, "--allowedTools", TOOLS, "--system-prompt-file",
    ]
    .map(String::from)
    .to_vec();
    out.push(system_file.to_string_lossy().into_owned());
    if !model.is_empty() {
        out.extend(["--model".to_string(), model.to_string()]);
    }
    out
}

/// What goes to its input: the plain turns so far, then the new question.
pub fn prompt(history: &[Value], question: &str) -> String {
    if history.is_empty() {
        return question.to_string();
    }
    let mut out = String::from("The conversation so far:\n\n");
    for m in history {
        let who = if m["role"] == "user" { "User" } else { "Assistant" };
        out.push_str(&format!("{who}: {}\n\n", m["content"].as_str().unwrap_or("")));
    }
    out.push_str(&format!("The user's new message:\n\n{question}"));
    out
}

/// The answer from `--output-format json`: the last line that is its result.
pub fn parse(out: &str) -> Result<String, String> {
    let result = out
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str::<Value>(l.trim()).ok().filter(|v| v["type"] == "result"))
        .ok_or_else(|| t("Unexpected API response."))?;
    let text = result["result"].as_str().unwrap_or("").trim().to_string();
    if result["is_error"] == true {
        return Err(format!("Claude Code: {text}"));
    }
    if text.is_empty() {
        return Err(t("No response text."));
    }
    Ok(text)
}

/// True when a Claude API error means the key has no credit left (402, a low
/// credit balance, billing or quota), which Claude Code can answer instead.
pub fn out_of_credit(err: &str) -> bool {
    let e = err.to_lowercase();
    ["claude api 402", "credit balance", "billing", "insufficient", "quota"].iter().any(|w| e.contains(w))
}

/// One chat turn through the CLI.
pub async fn send(chat: &Chat, model: &str, query: String, context: Option<ChatContext>) -> Result<ChatReply, String> {
    let exe = find().ok_or_else(not_installed)?;
    let turn = chat.begin(PROVIDER);
    let question = chat::plain_question(turn.first, context.as_ref(), &query);
    // A dropped text file goes along with the first question, as with Claude.
    let mut asked = question.clone();
    if let Some(ChatContext::File { path, .. }) = context.as_ref().filter(|_| turn.first) {
        if let Some((text, _)) = chat::read_text(path) {
            asked = format!("File contents:\n{text}\n\n{question}");
        }
    }
    let input = prompt(&turn.history, &asked);
    let args = args(model, &base_dir().join("system.txt"));
    let system = chat.system_prompt(true);
    let text = tauri::async_runtime::spawn_blocking(move || run(&exe, &args, &system, &input))
        .await
        .map_err(|e| e.to_string())??;
    chat.commit(
        &turn,
        json!({ "role": "user", "content": question }),
        json!({ "role": "assistant", "content": text }),
        &question,
        &text,
    );
    Ok(ChatReply::text(text))
}

fn base_dir() -> PathBuf {
    std::env::temp_dir().join("coucou-claude-code")
}

fn run(exe: &Path, args: &[String], system: &str, input: &str) -> Result<String, String> {
    let work = base_dir().join("work");
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    std::fs::write(base_dir().join("system.txt"), system).map_err(|e| e.to_string())?;
    let mut cmd = platform::cli_command(exe);
    cmd.args(args).current_dir(&work).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = cmd.spawn().map_err(|_| not_installed())?;
    *RUNNING.lock().unwrap() = Some(child.id());
    let (mut stdin, stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
    let input = input.to_string();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
        drop(stdin);
        let mut out = String::new();
        let _ = stdout.take(MAX_OUTPUT).read_to_string(&mut out);
        let _ = tx.send(out);
    });
    let out = rx.recv_timeout(TIMEOUT);
    let ours = RUNNING.lock().unwrap().take_if(|pid| *pid == child.id()).is_some();
    if out.is_err() && ours {
        platform::kill_tree(child.id());
    }
    let _ = child.kill();
    let _ = child.wait();
    parse(&out.map_err(|_| t("Claude Code took too long to answer."))?)
}

/// The models it offers: the CLI's aliases for the latest of each.
pub fn models() -> Result<Vec<ModelInfo>, String> {
    find().ok_or_else(not_installed)?;
    Ok([("sonnet", "Sonnet"), ("opus", "Opus"), ("haiku", "Haiku")]
        .map(|(id, label)| ModelInfo { id: id.into(), label: format!("Claude {label}") })
        .to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_runs_headless_with_web_tools_only_and_no_saved_session() {
        let a = args("opus", Path::new("/tmp/system.txt"));
        let at = |flag: &str| a.iter().position(|x| x == flag).map(|i| a[i + 1].as_str());
        assert_eq!(a[0], "-p");
        assert_eq!(at("--output-format"), Some("json"));
        assert_eq!(at("--tools"), Some("WebSearch,WebFetch"));
        assert_eq!(at("--allowedTools"), Some("WebSearch,WebFetch"));
        assert_eq!(at("--system-prompt-file"), Some("/tmp/system.txt"));
        assert_eq!(at("--model"), Some("opus"));
        assert!(a.contains(&"--no-session-persistence".into()) && a.contains(&"--safe-mode".into()));
        assert!(!args("", Path::new("s")).contains(&"--model".into()));
    }

    #[test]
    fn the_conversation_is_folded_into_its_input() {
        assert_eq!(prompt(&[], "hi"), "hi");
        let history = [json!({"role":"user","content":"q1"}), json!({"role":"assistant","content":"a1"})];
        assert_eq!(
            prompt(&history, "q2"),
            "The conversation so far:\n\nUser: q1\n\nAssistant: a1\n\nThe user's new message:\n\nq2"
        );
    }

    #[test]
    fn the_answer_is_the_result_line_and_an_error_says_so() {
        let ok = r#"{"type":"result","is_error":false,"result":" Paris.\n{\"mood\":\"happy\"} "}"#;
        assert_eq!(parse(&format!("noise\n{ok}\n")), Ok("Paris.\n{\"mood\":\"happy\"}".into()));
        let err = r#"{"type":"result","is_error":true,"result":"Not logged in"}"#;
        assert_eq!(parse(err), Err("Claude Code: Not logged in".into()));
        assert!(parse("").is_err());
        assert!(parse(r#"{"type":"result","is_error":false,"result":""}"#).is_err());
        assert!(parse(r#"{"type":"system"}"#).is_err());
    }

    #[test]
    fn only_credit_and_billing_errors_fall_back() {
        assert!(out_of_credit("Claude API 402 Payment Required: x"));
        assert!(out_of_credit("Claude API 400 Bad Request: Your credit balance is too low to access the Anthropic API."));
        assert!(out_of_credit("Claude API 429 Too Many Requests: billing limit reached"));
        assert!(!out_of_credit("Claude API 401 Unauthorized: invalid x-api-key"));
        assert!(!out_of_credit("Claude API 529: Overloaded"));
        assert!(!out_of_credit("Network error: timed out"));
    }
}
