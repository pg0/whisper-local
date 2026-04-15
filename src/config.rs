use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub mic_name: String,
    pub whisper: WhisperCfg,
    pub enable_speaker_detection: bool,
    /// ISO code forwarded as `language` form field. Empty = auto-detect.
    pub language: String,
    /// When true, press Enter after every transcript written back.
    pub newline_feed: bool,
    /// When true, after you've held the chord for `auto_hold_secs` seconds, keep
    /// recording on its own so you can let go.
    pub auto_hold: bool,
    pub auto_hold_secs: f32,
    /// Always-on: RMS below this counts as silence.
    pub silence_rms_threshold: f32,
    /// Silence-duration for Stop: pause this long → end session.
    pub stop_silence_secs: f32,
    /// When true, stop and transcribe once silence is detected (one-shot).
    pub auto_stop: bool,
    /// When true, after the transcript is typed, restart recording in latched
    /// state and wait for the next silence. Press the chord to break the loop.
    pub continuous: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mic_name: String::new(),
            whisper: WhisperCfg::default(),
            enable_speaker_detection: false,
            language: String::new(),
            newline_feed: false,
            auto_hold: false,
            auto_hold_secs: 2.0,
            silence_rms_threshold: 0.01,
            stop_silence_secs: 5.0,
            auto_stop: false,
            continuous: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WhisperCfg {
    pub base_url: String,
    pub start_url: String,
    pub model_param: String,
    pub response_format: String,
    pub request_timeout_secs: u64,
    pub start_body: WhisperStartBody,
}

impl Default for WhisperCfg {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:10010".into(),
            start_url: "http://localhost:9999/start".into(),
            model_param: "whisper-1".into(),
            response_format: "json".into(),
            request_timeout_secs: 30,
            start_body: WhisperStartBody::default(),
        }
    }
}

impl WhisperCfg {
    pub fn transcribe_url(&self) -> String {
        format!("{}/v1/audio/transcriptions", self.base_url.trim_end_matches('/'))
    }
    pub fn health_url(&self) -> String {
        format!("{}/health", self.base_url.trim_end_matches('/'))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WhisperStartBody {
    #[serde(rename = "type")]
    pub kind: String,
    pub model_id: String,
    pub model: String,
    pub port: u16,
    pub gpu: Vec<u32>,
    pub ctx: u32,
}

impl Default for WhisperStartBody {
    fn default() -> Self {
        Self {
            kind: "whisper".into(),
            model_id: "deepdml/faster-whisper-large-v3-turbo-ct2".into(),
            model: "deepdml/faster-whisper-large-v3-turbo-ct2".into(),
            port: 10010,
            gpu: vec![0],
            ctx: 0,
        }
    }
}

/// Native-script labels + ISO code, used by both the settings window and the
/// transcribe-file Language combo. Empty code = auto-detect.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("", "Auto-detect"),
    ("en", "English (en)"),
    ("de", "Deutsch (de)"),
    ("fr", "Français (fr)"),
    ("es", "Español (es)"),
    ("it", "Italiano (it)"),
    ("nl", "Nederlands (nl)"),
    ("pt", "Português (pt)"),
    ("pl", "Polski (pl)"),
    ("ru", "Русский (ru)"),
    ("zh", "中文 (zh)"),
    ("ja", "日本語 (ja)"),
    ("ko", "한국어 (ko)"),
];

pub fn config_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().context("no APPDATA")?;
    Ok(base.join("whisper-local"))
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            let c = Config::default();
            c.save()?;
            return Ok(c);
        }
        let s = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let c: Config = toml::from_str(&s).with_context(|| "parse config.toml")?;
        Ok(c)
    }

    pub fn save(&self) -> Result<()> {
        let dir = config_dir()?;
        fs::create_dir_all(&dir)?;
        let s = toml::to_string_pretty(self)?;
        fs::write(config_path()?, s)?;
        Ok(())
    }
}
