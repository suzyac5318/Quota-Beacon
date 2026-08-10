#[cfg(windows)]
mod windows_impl {
    use std::{
        collections::{HashMap, HashSet},
        fs::{self, File},
        io::{BufRead, BufReader, Seek, SeekFrom},
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant},
    };

    use tauri::{AppHandle, Emitter};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HWND, LPARAM},
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetForegroundWindow, GetWindowThreadProcessId, IsWindowVisible,
        },
    };

    use crate::token_usage;

    const ACTIVE_POLL_INTERVAL: Duration = Duration::from_millis(500);
    const BACKGROUND_POLL_INTERVAL: Duration = Duration::from_secs(1);
    const DORMANT_POLL_INTERVAL: Duration = Duration::from_secs(2);
    const ACTIVE_TOKEN_REFRESH_INTERVAL: Duration = Duration::from_secs(2);
    const BACKGROUND_TOKEN_REFRESH_INTERVAL: Duration = Duration::from_secs(10);
    const FULL_SESSION_DISCOVERY_INTERVAL: Duration = Duration::from_secs(10);
    const LOG_DISCOVERY_INTERVAL: Duration = Duration::from_secs(5);

    #[derive(Default)]
    struct LogTail {
        path: Option<PathBuf>,
        offset: u64,
        last_discovery: Option<Instant>,
    }

    #[derive(Clone, Debug)]
    struct ActivityEvent {
        renderer_window_id: String,
        conversation_id: String,
    }

    unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> i32 {
        if IsWindowVisible(hwnd) != 0 && is_codex_window(hwnd) {
            let windows = &mut *(lparam as *mut Vec<isize>);
            windows.push(hwnd as isize);
        }
        1
    }

    unsafe fn is_codex_window(hwnd: HWND) -> bool {
        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, &mut process_id);
        if process_id == 0 {
            return false;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
        if process.is_null() {
            return false;
        }
        let mut buffer = vec![0u16; 1024];
        let mut length = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) != 0;
        CloseHandle(process);
        if !ok {
            return false;
        }
        let path = String::from_utf16_lossy(&buffer[..length as usize]).to_ascii_lowercase();
        path.ends_with("\\chatgpt.exe") && path.contains("\\openai.codex_")
    }

    fn enumerate_codex_windows() -> Vec<isize> {
        let mut windows = Vec::new();
        unsafe {
            EnumWindows(Some(enum_window), &mut windows as *mut Vec<isize> as LPARAM);
        }
        windows
    }

    fn logs_root() -> Option<PathBuf> {
        let packages = dirs::data_local_dir()?.join("Packages");
        fs::read_dir(packages).ok()?.flatten().find_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("OpenAI.Codex_") {
                return None;
            }
            let path = entry.path().join("LocalCache/Local/Codex/Logs");
            path.is_dir().then_some(path)
        })
    }

    fn newest_log(directory: &Path) -> Option<PathBuf> {
        fn visit(directory: &Path, newest: &mut Option<(std::time::SystemTime, PathBuf)>) {
            let Ok(entries) = fs::read_dir(directory) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    visit(&path, newest);
                } else if path.extension().and_then(|value| value.to_str()) == Some("log") {
                    let modified = entry.metadata().and_then(|value| value.modified()).ok();
                    if let Some(modified) = modified {
                        if newest.as_ref().is_none_or(|current| modified > current.0) {
                            *newest = Some((modified, path));
                        }
                    }
                }
            }
        }
        let mut newest = None;
        visit(directory, &mut newest);
        newest.map(|(_, path)| path)
    }

    fn field(line: &str, name: &str) -> Option<String> {
        let start = line.find(name)? + name.len();
        let value = &line[start..];
        let end = value.find(char::is_whitespace).unwrap_or(value.len());
        Some(value[..end].trim_matches('"').to_string())
    }

    fn activity_event(line: &str) -> Option<ActivityEvent> {
        if !line.contains("rendererWindowFocused=true") {
            return None;
        }
        let explicitly_active = line.contains("thread_stream_view_activity_changed active=true");
        let conversation_created = line.contains("Conversation created");
        let thread_activity = line.contains("threadId=");
        if !explicitly_active && !conversation_created && !thread_activity {
            return None;
        }
        Some(ActivityEvent {
            renderer_window_id: field(line, "rendererWindowId=")?,
            conversation_id: field(line, "conversationId=").or_else(|| field(line, "threadId="))?,
        })
    }

    impl LogTail {
        fn read_events(&mut self) -> Vec<ActivityEvent> {
            let should_discover = self.path.is_none()
                || self
                    .last_discovery
                    .is_none_or(|last| last.elapsed() >= LOG_DISCOVERY_INTERVAL);
            let path = if should_discover {
                self.last_discovery = Some(Instant::now());
                logs_root()
                    .and_then(|root| newest_log(&root))
                    .or_else(|| self.path.clone())
            } else {
                self.path.clone()
            };
            let Some(path) = path else { return Vec::new() };
            let initial_read = self.path.as_ref() != Some(&path);
            if initial_read {
                self.path = Some(path.clone());
                self.offset = fs::metadata(&path)
                    .map(|metadata| metadata.len().saturating_sub(8 * 1024 * 1024))
                    .unwrap_or(0);
            }
            let Ok(mut file) = File::open(path) else {
                return Vec::new();
            };
            if file.seek(SeekFrom::Start(self.offset)).is_err() {
                return Vec::new();
            }
            let mut reader = BufReader::new(file);
            if self.offset > 0 {
                let mut partial = String::new();
                let _ = reader.read_line(&mut partial);
            }
            let mut events = Vec::new();
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                if let Some(event) = activity_event(&line) {
                    events.push(event);
                }
                line.clear();
            }
            if let Ok(offset) = reader.stream_position() {
                self.offset = offset;
            }
            if initial_read {
                events.pop().into_iter().collect()
            } else {
                events
            }
        }
    }

    pub fn start(app: AppHandle, cache: Arc<Mutex<token_usage::TokenUsageCache>>) {
        thread::spawn(move || {
            let mut log_tail = LogTail::default();
            let mut renderer_to_hwnd: HashMap<String, isize> = HashMap::new();
            let mut conversations: HashMap<isize, String> = HashMap::new();
            let mut latest_activity: Option<ActivityEvent> = None;
            let mut last_active_hwnd = None;
            let mut last_conversation_refresh: Option<Instant> = None;
            let mut last_full_session_discovery: Option<Instant> = None;

            loop {
                let window_handles: HashSet<isize> =
                    enumerate_codex_windows().into_iter().collect();
                let foreground = unsafe { GetForegroundWindow() } as isize;
                let codex_app_active = window_handles.contains(&foreground);
                if codex_app_active {
                    last_active_hwnd = Some(foreground);
                }

                let mut conversation_changed = false;
                for event in log_tail.read_events() {
                    latest_activity = Some(event.clone());
                    if codex_app_active {
                        renderer_to_hwnd.insert(event.renderer_window_id.clone(), foreground);
                    }
                    if let Some(hwnd) = renderer_to_hwnd.get(&event.renderer_window_id).copied() {
                        conversation_changed |=
                            conversations.get(&hwnd) != Some(&event.conversation_id);
                        conversations.insert(hwnd, event.conversation_id);
                    }
                }
                if codex_app_active && !conversations.contains_key(&foreground) {
                    if let Some(event) = latest_activity.as_ref() {
                        renderer_to_hwnd.insert(event.renderer_window_id.clone(), foreground);
                        conversations.insert(foreground, event.conversation_id.clone());
                        conversation_changed = true;
                    }
                }

                renderer_to_hwnd.retain(|_, hwnd| window_handles.contains(hwnd));
                conversations.retain(|hwnd, _| window_handles.contains(hwnd));
                if last_active_hwnd.is_some_and(|hwnd| !window_handles.contains(&hwnd)) {
                    last_active_hwnd = None;
                }

                let token_refresh_interval = if codex_app_active {
                    ACTIVE_TOKEN_REFRESH_INTERVAL
                } else {
                    BACKGROUND_TOKEN_REFRESH_INTERVAL
                };
                let token_refresh_due = conversation_changed
                    || last_conversation_refresh
                        .is_none_or(|last| last.elapsed() >= token_refresh_interval);
                if token_refresh_due {
                    let conversation_id = last_active_hwnd
                        .and_then(|hwnd| conversations.get(&hwnd))
                        .map(String::as_str);
                    let discover = conversation_changed
                        || last_full_session_discovery
                            .is_none_or(|last| last.elapsed() >= FULL_SESSION_DISCOVERY_INTERVAL);
                    let payload = token_usage::scan_conversation(&cache, conversation_id, discover);
                    let _ = app.emit_to("widget", "conversation-token-usage", payload);
                    last_conversation_refresh = Some(Instant::now());
                    if discover && conversation_id.is_some() {
                        last_full_session_discovery = Some(Instant::now());
                    }
                }
                let poll_interval = if codex_app_active {
                    ACTIVE_POLL_INTERVAL
                } else if window_handles.is_empty() {
                    DORMANT_POLL_INTERVAL
                } else {
                    BACKGROUND_POLL_INTERVAL
                };
                thread::sleep(poll_interval);
            }
        });
    }
}

#[cfg(windows)]
pub use windows_impl::start;

#[cfg(not(windows))]
pub fn start(
    _app: tauri::AppHandle,
    _cache: std::sync::Arc<std::sync::Mutex<crate::token_usage::TokenUsageCache>>,
) {
}
