//! A Model Context Protocol server that lets AI tools on this computer (Claude Code, Claude
//! Desktop, Cursor and the like) read the meetings kept here: their list, a search through
//! them, and one meeting in full. It only reads; nothing is written, sent or changed.
//!
//! It runs when the app is started with `--mcp`, talking MCP over its standard input and
//! output, and exits when the client closes them. `main` checks for the flag before the app
//! itself starts, so the single-instance plugin never hands it to a running app and no
//! window opens.
//!
//! On Windows the release build has no console (`windows_subsystem = "windows"`), but a
//! client that starts it with its standard input and output redirected to pipes, as MCP
//! clients do, hands it those pipes, and reading and writing them works as anywhere else.

use std::path::{Path, PathBuf};

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::meetings::{self, Meeting};

/// The flag that starts the server instead of the app.
pub const FLAG: &str = "--mcp";

/// The app's identifier, which names its data folder.
const IDENTIFIER: &str = "com.opentypeless.app";

/// Points the server at another data folder than the app's own. Tests use it.
const DATA_DIR_VAR: &str = "OT_MCP_DATA_DIR";

/// How many meetings a list gives when the client does not say, and the most it gives.
const LISTED: usize = 50;
const MOST_LISTED: usize = 500;

/// Whether the app was started to serve MCP rather than to open.
pub fn wanted(mut args: impl Iterator<Item = String>) -> bool {
    args.any(|arg| arg == FLAG)
}

/// The folder the app keeps its data in, the one Tauri's `app_data_dir` gives it.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(DATA_DIR_VAR) {
        return Some(PathBuf::from(dir));
    }
    dirs::data_dir().map(|dir| dir.join(IDENTIFIER))
}

/// What a client is told to run, for the settings to show: this program with the flag, as
/// most MCP clients write it in their configuration.
pub fn client_config() -> Value {
    let command = std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    json!({ "mcpServers": { "speech": { "command": command, "args": [FLAG] } } })
}

/// Serves MCP on standard input and output until the client closes them. Gives the exit
/// code.
pub fn serve() -> i32 {
    let Some(data) = data_dir() else {
        eprintln!("speech mcp: no data folder on this system");
        return 1;
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("speech mcp: {error}");
            return 1;
        }
    };
    let server = MeetingServer::new(data.join("meetings"));
    let served = runtime.block_on(async move {
        let service = server.serve(rmcp::transport::stdio()).await?;
        service.waiting().await?;
        anyhow::Ok(())
    });
    match served {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("speech mcp: {error}");
            1
        }
    }
}

/// A day and time in the computer's own time zone, from milliseconds since 1970.
fn when(started_at: u64) -> String {
    chrono::DateTime::from_timestamp_millis(started_at as i64)
        .map(|at| {
            at.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// A heading of the transcript naming a numbered speaker, split into the speaker's number,
/// the label and the time; None for any other line.
fn heading(line: &str) -> Option<(u32, &str, &str)> {
    let found = crate::meetings::heading(line)?;
    Some((found.number?, found.label, found.time))
}

/// The transcript with each speaker the user named given by that name.
pub fn with_names(meeting: &Meeting) -> String {
    meeting
        .transcript
        .lines()
        .map(|line| match heading(line) {
            Some((number, _, time)) => match meeting.names.get(&number).map(|name| name.trim()) {
                Some(name) if !name.is_empty() => format!("{name}  {time}"),
                _ => line.to_string(),
            },
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The meetings, newest first, those in `folder` alone where it is given, at most `limit`.
pub fn list_in(dir: &Path, folder: Option<&str>, limit: Option<usize>) -> Value {
    let folder = folder.map(str::trim).filter(|folder| !folder.is_empty());
    let shown: Vec<Value> = meetings::list_in(dir)
        .into_iter()
        .filter(|meeting| folder.is_none_or(|folder| meeting.folder == folder))
        .take(limit.unwrap_or(LISTED).clamp(1, MOST_LISTED))
        .map(|meeting| {
            json!({
                "id": meeting.id,
                "title": meeting.title,
                "date": when(meeting.started_at),
                "started_at": meeting.started_at,
                "duration_s": meeting.duration_s,
                "folder": meeting.folder,
                "has_minutes": meeting.has_minutes,
                "preview": meeting.preview,
            })
        })
        .collect();
    Value::Array(shown)
}

/// The meetings that hold every word of `query`, newest first, each with the words around
/// the first place found.
pub fn search_in(dir: &Path, query: &str) -> Value {
    let found: Vec<Value> = meetings::search_in(dir, query)
        .into_iter()
        .map(|meeting| {
            json!({
                "id": meeting.id,
                "title": meeting.title,
                "date": when(meeting.started_at),
                "duration_s": meeting.duration_s,
                "found": meeting.found,
            })
        })
        .collect();
    Value::Array(found)
}

/// One meeting in full, laid out in Markdown: its name and day, its minutes, the notes
/// taken during it and what was said, with the speakers the user named by name.
pub fn meeting_in(dir: &Path, id: &str) -> Result<String, String> {
    let meeting = meetings::get_in(dir, id)?;
    let title = match meeting.title.trim() {
        "" => "Meeting",
        title => title,
    };
    let mut out = format!(
        "# {title}\n\nid: {}\ndate: {}\n",
        meeting.id,
        when(meeting.started_at)
    );
    if meeting.duration_s > 0 {
        out.push_str(&format!(
            "duration: {} min\n",
            meeting.duration_s.div_ceil(60)
        ));
    }
    if !meeting.folder.trim().is_empty() {
        out.push_str(&format!("folder: {}\n", meeting.folder.trim()));
    }
    for (heading, text) in [
        ("Minutes", meeting.minutes.trim().to_string()),
        (
            "Notes taken during the meeting",
            meeting.notes.trim().to_string(),
        ),
        ("Transcript", with_names(&meeting).trim().to_string()),
    ] {
        if !text.is_empty() {
            out.push_str(&format!("\n## {heading}\n\n{text}\n"));
        }
    }
    Ok(out)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListRequest {
    /// Only the meetings in this folder.
    #[serde(default)]
    pub folder: Option<String>,
    /// The most meetings to give, newest first (50 when not given, at most 500).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchRequest {
    /// The words to look for; a meeting must hold every one of them.
    pub query: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MeetingRequest {
    /// The meeting's id, as list_meetings or search_meetings give it.
    pub id: String,
}

/// The server, reading the meetings kept in one folder.
#[derive(Debug, Clone)]
pub struct MeetingServer {
    dir: PathBuf,
    tool_router: ToolRouter<Self>,
}

impl MeetingServer {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            tool_router: Self::tool_router(),
        }
    }
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

#[tool_router(router = tool_router)]
impl MeetingServer {
    #[tool(
        description = "List the meetings recorded or transcribed in the Speech app on this \
computer, newest first: id, title, date, length, folder, whether minutes were written, and \
the first words said.",
        annotations(read_only_hint = true)
    )]
    async fn list_meetings(&self, Parameters(request): Parameters<ListRequest>) -> String {
        pretty(&list_in(
            &self.dir,
            request.folder.as_deref(),
            request.limit,
        ))
    }

    #[tool(
        description = "Find the meetings whose transcript, notes, minutes or title hold every \
word of the query, newest first, with the words around the first place found.",
        annotations(read_only_hint = true)
    )]
    async fn search_meetings(&self, Parameters(request): Parameters<SearchRequest>) -> String {
        pretty(&search_in(&self.dir, &request.query))
    }

    #[tool(
        description = "Read one meeting in full: its title and date, its minutes, the notes \
taken during it, and its transcript with each speaker under their name where one was given.",
        annotations(read_only_hint = true)
    )]
    async fn get_meeting(
        &self,
        Parameters(request): Parameters<MeetingRequest>,
    ) -> Result<String, ErrorData> {
        meeting_in(&self.dir, request.id.trim()).map_err(|error| {
            ErrorData::invalid_params(format!("no meeting with id {}: {error}", request.id), None)
        })
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MeetingServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("speech", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Read-only access to the meetings kept by the Speech app on this computer. \
Use search_meetings or list_meetings to find a meeting, then get_meeting to read it.",
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ot-mcp-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn keep(dir: &Path, meeting: &Meeting) {
        std::fs::write(
            dir.join(format!("{}.json", meeting.id)),
            serde_json::to_vec(meeting).unwrap(),
        )
        .unwrap();
    }

    fn two_meetings(dir: &Path) {
        keep(
            dir,
            &Meeting {
                id: "m-1".into(),
                started_at: 1_000,
                title: "Garden plans".into(),
                transcript: "Speaker 1  00:00\nWe plant the roses in spring.\n\nSpeaker 2  00:05\nAnd the tulips in autumn.".into(),
                names: HashMap::from([(1, "Ann".to_string())]),
                notes: "[00:01] roses".into(),
                minutes: "## Decisions\n- Roses in spring [00:00]".into(),
                folder: "Home".into(),
                duration_s: 90,
                ..Default::default()
            },
        );
        keep(
            dir,
            &Meeting {
                id: "m-2".into(),
                started_at: 2_000,
                title: "Bake sale".into(),
                transcript: "Speaker 1  00:00\nTwelve cakes and a pie.".into(),
                ..Default::default()
            },
        );
    }

    #[test]
    fn only_the_flag_starts_the_server() {
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        assert!(wanted(args(&["speech", "--mcp"]).into_iter()));
        assert!(!wanted(args(&["speech"]).into_iter()));
        assert!(!wanted(args(&["speech", "--mcpx"]).into_iter()));
    }

    #[test]
    fn meetings_are_listed_newest_first_and_by_folder() {
        let dir = folder("list");
        two_meetings(&dir);
        let all = list_in(&dir, None, None);
        let ids: Vec<&str> = all
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["m-2", "m-1"]);
        assert_eq!(all[1]["has_minutes"], true);
        assert_eq!(all[1]["folder"], "Home");
        let home = list_in(&dir, Some("Home"), None);
        assert_eq!(home.as_array().unwrap().len(), 1);
        assert_eq!(list_in(&dir, None, Some(1)).as_array().unwrap().len(), 1);
        // Nothing kept is nothing listed, not a failure.
        assert_eq!(list_in(&dir.join("none"), None, None), json!([]));
    }

    #[test]
    fn a_search_finds_the_meetings_that_hold_every_word() {
        let dir = folder("search");
        two_meetings(&dir);
        let found = search_in(&dir, "tulips autumn");
        assert_eq!(found.as_array().unwrap().len(), 1);
        assert_eq!(found[0]["id"], "m-1");
        assert!(found[0]["found"].as_str().unwrap().contains("tulips"));
        assert_eq!(search_in(&dir, "tulips cakes"), json!([]));
    }

    #[test]
    fn a_meeting_is_read_with_its_speakers_named() {
        let dir = folder("get");
        two_meetings(&dir);
        let read = meeting_in(&dir, "m-1").unwrap();
        assert!(read.starts_with("# Garden plans\n"));
        assert!(read.contains("## Minutes\n\n## Decisions\n- Roses in spring [00:00]"));
        assert!(read.contains("## Notes taken during the meeting\n\n[00:01] roses"));
        assert!(read.contains("Ann  00:00\nWe plant the roses in spring."));
        // A speaker without a name keeps their number.
        assert!(read.contains("Speaker 2  00:05\nAnd the tulips in autumn."));
        assert!(meeting_in(&dir, "m-9").is_err());
        // An id that could name another file is refused.
        assert!(meeting_in(&dir, "../m-1").is_err());
    }

    #[test]
    fn only_a_heading_has_its_speaker_named() {
        assert_eq!(heading("Speaker 2  01:05"), Some((2, "Speaker", "01:05")));
        assert_eq!(
            heading("说话人 12  1:02:03"),
            Some((12, "说话人", "1:02:03"))
        );
        assert_eq!(heading("We met at 10  10:30 sharp"), None);
        assert_eq!(heading("Speaker  00:00"), None);
    }

    #[test]
    fn the_client_is_told_to_run_this_program_with_the_flag() {
        let config = client_config();
        assert_eq!(config["mcpServers"]["speech"]["args"], json!(["--mcp"]));
        assert!(!config["mcpServers"]["speech"]["command"]
            .as_str()
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn the_server_lists_its_tools_and_answers_a_call() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let dir = folder("serve");
        two_meetings(&dir);
        let (server_side, client_side) = tokio::io::duplex(64 * 1024);
        let server = MeetingServer::new(dir);
        tokio::spawn(async move {
            if let Ok(service) = server.serve(server_side).await {
                let _ = service.waiting().await;
            }
        });
        let (read, mut write) = tokio::io::split(client_side);
        let mut lines = BufReader::new(read).lines();
        let ask = |message: Value| {
            let mut line = message.to_string();
            line.push('\n');
            line
        };
        let send = [
            ask(
                json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
            ),
            ask(json!({"jsonrpc":"2.0","method":"notifications/initialized"})),
            ask(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})),
            ask(
                json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_meeting","arguments":{"id":"m-2"}}}),
            ),
        ];
        let mut answers = Vec::new();
        for (index, message) in send.iter().enumerate() {
            write.write_all(message.as_bytes()).await.unwrap();
            if index == 1 {
                continue;
            }
            let line = lines.next_line().await.unwrap().unwrap();
            answers.push(serde_json::from_str::<Value>(&line).unwrap());
        }
        assert_eq!(answers[0]["result"]["serverInfo"]["name"], "speech");
        let mut tools: Vec<&str> = answers[1]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        tools.sort();
        assert_eq!(tools, ["get_meeting", "list_meetings", "search_meetings"]);
        assert!(answers[1]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| tool["annotations"]["readOnlyHint"] == true));
        let text = answers[2]["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.starts_with("# Bake sale"));
        assert!(text.contains("Twelve cakes and a pie."));
    }
}
