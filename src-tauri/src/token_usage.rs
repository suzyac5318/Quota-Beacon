use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{BufRead, BufReader},
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
        input_tokens: value.get("input_tokens").and_then(Value::as_u64).unwrap_or(0),
        cached_input_tokens: value
            .get("cached_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output_tokens: value.get("output_tokens").and_then(Value::as_u64).unwrap_or(0),
        reasoning_output_tokens: value
            .get("reasoning_output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        total_tokens: value.get("total_tokens").and_then(Value::as_u64).unwrap_or(0),
        ..SessionUsage::default()
    }
}

fn parse_session<R: BufRead>(reader: R, fallback_id: String) -> SessionUsage {
    let mut session_id = None;
    let mut usage = SessionUsage::default();

    for line in reader.lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if session_id.is_none() && value.get("type").and_then(Value::as_str) == Some("session_meta") {
            session_id = value
                .pointer("/payload/id")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        let Some(total) = value.pointer("/payload/info/total_token_usage") else {
            continue;
        };
        let candidate = read_tokens(total);
        if candidate.total_tokens >= usage.total_tokens {
            usage = candidate;
        }
    }

    usage.session_id = session_id.unwrap_or(fallback_id);
    usage
}

fn parse_session_file(path: &Path) -> SessionUsage {
    let fallback_id = path.to_string_lossy().into_owned();
    File::open(path)
        .map(|file| parse_session(BufReader::new(file), fallback_id.clone()))
        .unwrap_or_else(|_| SessionUsage {
            session_id: fallback_id,
            ..SessionUsage::default()
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
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        let fingerprint = FileFingerprint {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        };
        let unchanged = cache
            .files
            .get(&path)
            .is_some_and(|cached| cached.fingerprint == fingerprint);
        if unchanged {
            continue;
        }
        cache.files.insert(
            path.clone(),
            CachedSessionFile {
                fingerprint,
                usage: parse_session_file(&path),
            },
        );
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
) -> ConversationTokenUsage {
    let Some(conversation_id) = conversation_id else {
        return ConversationTokenUsage {
            conversation_id: None,
            total_tokens: None,
        };
    };

    let _ = scan(cache);
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
        io::Cursor,
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
}
