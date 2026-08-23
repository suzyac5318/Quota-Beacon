use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub remaining_percent: f64,
    pub resets_at: Option<String>,
    pub window_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshot {
    pub provider: String,
    pub display_name: String,
    pub plan: Option<String>,
    pub short_window: Option<UsageWindow>,
    pub weekly_window: Option<UsageWindow>,
    pub reset_credits: Option<u64>,
    pub reset_credit_expires_at: Vec<String>,
    pub updated_at: String,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageSummary {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
    pub session_count: u64,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationTokenUsage {
    pub conversation_id: Option<String>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountWeeklyQuota {
    pub profile_id: String,
    pub remaining_percent: Option<f64>,
    pub status: String,
    pub message: Option<String>,
}

impl ProviderSnapshot {
    pub fn failure(status: &str, message: &str) -> Self {
        Self {
            provider: "codex".into(),
            display_name: "CODEX".into(),
            plan: None,
            short_window: None,
            weekly_window: None,
            reset_credits: None,
            reset_credit_expires_at: Vec::new(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            status: status.into(),
            message: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetPreferences {
    pub locked: bool,
    #[serde(default = "default_always_on_top")]
    pub always_on_top: bool,
    pub pinned_provider: Option<String>,
    pub auto_rotate_seconds: u64,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_palette_colors")]
    pub palette_colors: Vec<String>,
}

fn default_always_on_top() -> bool { true }
fn default_language() -> String { "zh-CN".into() }
fn default_palette_colors() -> Vec<String> {
    ["#eb5b58", "#f1a06f", "#f5d98f", "#e3f4b8", "#b9e4c9"]
        .into_iter()
        .map(String::from)
        .collect()
}

pub(crate) fn valid_palette_colors(colors: &[String]) -> bool {
    colors.len() == 5 && colors.iter().all(|color| {
        color.len() == 7
            && color.starts_with('#')
            && color[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

impl Default for WidgetPreferences {
    fn default() -> Self {
        Self {
            locked: false,
            always_on_top: true,
            pinned_provider: None,
            auto_rotate_seconds: 12,
            language: default_language(),
            palette_colors: default_palette_colors(),
        }
    }
}

impl WidgetPreferences {
    pub fn normalized(mut self) -> Self {
        self.auto_rotate_seconds = self.auto_rotate_seconds.clamp(5, 300);
        if self.pinned_provider.as_deref() != Some("codex") {
            self.pinned_provider = None;
        }
        if self.language != "en" && self.language != "zh-CN" {
            self.language = default_language();
        }
        if !valid_palette_colors(&self.palette_colors) {
            self.palette_colors = default_palette_colors();
        } else {
            self.palette_colors = self.palette_colors.iter().map(|color| color.to_ascii_lowercase()).collect();
        }
        self
    }
}
