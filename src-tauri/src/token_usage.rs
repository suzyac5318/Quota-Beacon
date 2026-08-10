use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};

use serde_json::Value;

use crate::models::{ConversationTokenUsage, TokenUsageSummary};

#[derive(Clone, Default)]
struct SessionUsage {
    session_id: String,
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    reasoning_output_tokens: u64,
    total_tokens: u64,
}

#[derive(Clone, PartialEq, Eq)]
struct FileFingerprint {
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Clone)]
struct CachedSessionFile {
    fingerprint: FileFingerprint,
    parsed_len: u64,
    usage: SessionUsage,
}

#[derive(Default)]
pub struct TokenUsageCache {
    files: HashMap<PathBuf, CachedSessionFile>,
}

fn usage_roots(home: &Path) -> [PathBuf; 3] {
    [
        home.join("sessions"),
        home.join("archived_sessions"),
        home.join("session-archive"),
    ]
}

fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))
}

fn collect_jsonl_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, files);
        } else if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
            files.push(path);
        }
    }
}

fn read_tokens(value: &Value) -> SessionUsage {
    SessionUsage {
        input_tokens: value
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cached_input_tokens: value
            .get("cached_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output_tokens: value
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        reasoning_output_tokens: value
            .get("reasoning_output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        total_tokens: value
            .get("total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        ..SessionUsage::default()
    }
}

fn apply_session_line(line: &str, usage: &mut SessionUsage) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return false;
    };
    if value.get("type").and_then(Value::as_str) == Some("session_meta") {
        if let Some(session_id) = value.pointer("/payload/id").and_then(Value::as_str) {
            usage.session_id = session_id.to_owned();
        }
    }
    if let Some(total) = value.pointer("/payload/info/total_token_usage") {
        let mut candidate = read_tokens(total);
        if candidate.total_tokens >= usage.total_tokens {
            candidate.session_id = usage.session_id.clone();
            *usage = candidate;
        }
    }
    true
}

#[cfg(test)]
fn parse_session<R: BufRead>(reader: R, fallback_id: String) -> SessionUsage {
    let mut usage = SessionUsage {
        session_id: fallback_id,
        ..SessionUsage::default()
    };

    for line in reader.lines().map_while(Result::ok) {
        apply_session_line(&line, &mut usage);
    }
    usage
}

fn parse_session_file_from(
    path: &Path,
    start: u64,
    previous: Option<SessionUsage>,
) -> (SessionUsage, u64) {
    let fallback_id = path.to_string_lossy().into_owned();
    let mut usage = previous.unwrap_or_else(|| SessionUsage {
        session_id: fallback_id.clone(),
        ..SessionUsage::default()
    });
    let Ok(mut file) = File::open(path) else {
        return (usage, start);
    };
    if file.seek(SeekFrom::Start(start)).is_err() {
        return (
            SessionUsage {
                session_id: fallback_id,
                ..SessionUsage::default()
            },
            0,
        );
    }
    let mut reader = BufReader::new(file);
    let mut parsed_len = start;
    let mut line = String::new();
    loop {
        let line_start = reader.stream_position().unwrap_or(parsed_len);
        line.clear();
        let Ok(read) = reader.read_line(&mut line) else {
            break;
        };
        if read == 0 {
            break;
        }
        let valid = apply_session_line(&line, &mut usage);
        if !valid && !line.ends_with('\n') {
            parsed_len = line_start;
            break;
        }
        parsed_len = reader.stream_position().unwrap_or(line_start + read as u64);
    }
    (usage, parsed_len)
}

fn refresh_cached_file(
    path: &Path,
    previous: Option<&CachedSessionFile>,
) -> Option<CachedSessionFile> {
    let metadata = fs::metadata(path).ok()?;
    let fingerprint = FileFingerprint {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    };
    if previous.is_some_and(|cached| cached.fingerprint == fingerprint) {
        return previous.cloned();
    }
    let append_only = previous.is_some_and(|cached| fingerprint.len > cached.fingerprint.len);
    let start = if append_only {
        previous.map(|cached| cached.parsed_len).unwrap_or(0)
    } else {
        0
    };
    let previous_usage = if append_only {
        previous.map(|cached| cached.usage.clone())
    } else {
        None
    };
    let (usage, parsed_len) = parse_session_file_from(path, start, previous_usage);
    Some(CachedSessionFile {
        fingerprint,
        parsed_len,
        usage,
    })
}

fn add_usage(total: &mut TokenUsageSummary, usage: &SessionUsage) {
    total.input_tokens = total.input_tokens.saturating_add(usage.input_tokens);
    total.cached_input_tokens = total
        .cached_input_tokens
        .saturating_add(usage.cached_input_tokens);
    total.output_tokens = total.output_tokens.saturating_add(usage.output_tokens);
    total.reasoning_output_tokens = total
        .reasoning_output_tokens
        .saturating_add(usage.reasoning_output_tokens);
    total.total_tokens = total.total_tokens.saturating_add(usage.total_tokens);
}

fn scan_home(cache: &Mutex<TokenUsageCache>, home: &Path) -> Result<TokenUsageSummary, String> {
    let roots = usage_roots(home);
    if roots.iter().all(|root| !root.exists()) {
        return Err("Codex session history was not found.".into());
    }

    let mut paths = Vec::new();
    for root in roots.iter().filter(|root| root.exists()) {
        collect_jsonl_files(root, &mut paths);
    }
    let active_paths: HashSet<PathBuf> = paths.iter().cloned().collect();
    let mut cache = cache
        .lock()
        .map_err(|_| "Token usage cache is unavailable.".to_string())?;
    cache.files.retain(|path, _| active_paths.contains(path));

    for path in paths {
        if let Some(refreshed) = refresh_cached_file(&path, cache.files.get(&path)) {
            cache.files.insert(path, refreshed);
        }
    }

    let mut sessions: HashMap<String, SessionUsage> = HashMap::new();
    for cached in cache.files.values() {
        if cached.usage.total_tokens == 0 {
            continue;
        }
        sessions
            .entry(cached.usage.session_id.clone())
            .and_modify(|current| {
                if cached.usage.total_tokens > current.total_tokens {
                    *current = cached.usage.clone();
                }
            })
            .or_insert_with(|| cached.usage.clone());
    }

    let mut summary = TokenUsageSummary {
        session_count: sessions.len() as u64,
        updated_at: chrono::Utc::now().to_rfc3339(),
        ..TokenUsageSummary::default()
    };
    for usage in sessions.values() {
        add_usage(&mut summary, usage);
    }
    Ok(summary)
}

pub fn scan(cache: &Mutex<TokenUsageCache>) -> Result<TokenUsageSummary, String> {
    let home = codex_home().ok_or_else(|| "Codex home directory was not found.".to_string())?;
    scan_home(cache, &home)
}

pub fn scan_conversation(
    cache: &Mutex<TokenUsageCache>,
    conversation_id: Option<&str>,
    discover: bool,
) -> ConversationTokenUsage {
    let Some(conversation_id) = conversation_id else {
        return ConversationTokenUsage {
            conversation_id: None,
            total_tokens: None,
        };
    };

    let refreshed_cached_files = if discover {
        false
    } else {
        cache.lock().ok().is_some_and(|mut cache| {
            let paths: Vec<PathBuf> = cache
                .files
                .iter()
                .filter(|(_, cached)| cached.usage.session_id == conversation_id)
                .map(|(path, _)| path.clone())
                .collect();
            for path in &paths {
                if let Some(refreshed) = refresh_cached_file(path, cache.files.get(path)) {
                    cache.files.insert(path.clone(), refreshed);
                }
            }
            !paths.is_empty()
        })
    };
    if discover || !refreshed_cached_files {
        let _ = scan(cache);
    }
    let total_tokens = cache.lock().ok().and_then(|cache| {
        cache
            .files
            .values()
            .filter(|cached| cached.usage.session_id == conversation_id)
            .map(|cached| cached.usage.total_tokens)
            .max()
            .filter(|total| *total > 0)
    });
    ConversationTokenUsage {
        conversation_id: Some(conversation_id.to_string()),
        total_tokens,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Cursor, Write},
        sync::Mutex,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{parse_session, scan_home, TokenUsageCache};

    const SESSION_LOG: &str = r#"{"type":"session_meta","payload":{"id":"session-1"}}
{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":250,"cached_input_tokens":80,"output_tokens":20,"reasoning_output_tokens":5,"total_tokens":270}}}}"#;

    #[test]
    fn takes_latest_cumulative_usage_without_double_counting_events() {
        let input = r#"{"type":"session_meta","payload":{"id":"session-1"}}
{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":10,"reasoning_output_tokens":2,"total_tokens":110},"last_token_usage":{"total_tokens":110}}}}
{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":250,"cached_input_tokens":80,"output_tokens":20,"reasoning_output_tokens":5,"total_tokens":270},"last_token_usage":{"total_tokens":160}}}}"#;
        let usage = parse_session(Cursor::new(input), "fallback".into());

        assert_eq!(usage.session_id, "session-1");
        assert_eq!(usage.input_tokens, 250);
        assert_eq!(usage.cached_input_tokens, 80);
        assert_eq!(usage.output_tokens, 20);
        assert_eq!(usage.reasoning_output_tokens, 5);
        assert_eq!(usage.total_tokens, 270);
    }

    #[test]
    fn ignores_malformed_lines_and_uses_fallback_id() {
        let input = r#"not-json
{"type":"event_msg","payload":{"info":{"total_token_usage":{"total_tokens":42}}}}"#;
        let usage = parse_session(Cursor::new(input), "fallback".into());

        assert_eq!(usage.session_id, "fallback");
        assert_eq!(usage.total_tokens, 42);
    }

    #[test]
    fn keeps_all_time_usage_when_session_files_move_to_session_archive() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let home = std::env::temp_dir().join(format!(
            "quota-beacon-token-usage-{}-{unique}",
            std::process::id()
        ));
        let sessions = home.join("sessions");
        let archived = home.join("session-archive/stage2/2026/07/26");
        fs::create_dir_all(&sessions).expect("sessions directory should be created");
        let original = sessions.join("session.jsonl");
        fs::write(&original, SESSION_LOG).expect("session log should be written");

        let cache = Mutex::new(TokenUsageCache::default());
        let before = scan_home(&cache, &home).expect("initial history scan should succeed");

        fs::create_dir_all(&archived).expect("session archive should be created");
        fs::rename(&original, archived.join("session.jsonl"))
            .expect("session log should move into the archive");
        let after = scan_home(&cache, &home).expect("archived history scan should succeed");

        assert_eq!(before.total_tokens, 270);
        assert_eq!(after.total_tokens, before.total_tokens);
        assert_eq!(after.session_count, before.session_count);

        fs::remove_dir_all(&home).expect("temporary history should be removed");
    }

    #[test]
    fn incrementally_refreshes_a_growing_session_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let home = std::env::temp_dir().join(format!(
            "quota-beacon-token-growth-{}-{unique}",
            std::process::id()
        ));
        let sessions = home.join("sessions");
        fs::create_dir_all(&sessions).expect("sessions directory should be created");
        let session = sessions.join("session.jsonl");
        fs::write(
            &session,
            r#"{"type":"session_meta","payload":{"id":"session-1"}}
{"type":"event_msg","payload":{"info":{"total_token_usage":{"total_tokens":110}}}}
"#,
        )
        .expect("initial session log should be written");

        let cache = Mutex::new(TokenUsageCache::default());
        let before = scan_home(&cache, &home).expect("initial history scan should succeed");
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&session)
            .expect("session log should reopen for append");
        writeln!(
            file,
            r#"{{"type":"event_msg","payload":{{"info":{{"total_token_usage":{{"total_tokens":270}}}}}}}}"#
        )
        .expect("new cumulative usage should append");
        file.sync_all().expect("appended session log should flush");

        let after = scan_home(&cache, &home).expect("growing history scan should succeed");

        assert_eq!(before.total_tokens, 110);
        assert_eq!(after.total_tokens, 270);
        assert_eq!(after.session_count, 1);

        fs::remove_dir_all(&home).expect("temporary history should be removed");
    }
}
