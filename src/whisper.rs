use crate::config::WhisperCfg;
use anyhow::{anyhow, Context, Result};
use reqwest::blocking::{multipart, Client};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct Segment {
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: f64,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub speaker: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TranscribeResult {
    pub text: String,
    pub duration: Option<f64>,
    pub segments: Vec<Segment>,
    pub speaker_count: usize,
}

pub fn transcribe(wav: &[u8], language: &str, cfg: &WhisperCfg) -> Result<String> {
    transcribe_file_bytes(wav, "audio.wav", cfg.request_timeout_secs, language, cfg)
}

/// Transcribe arbitrary audio/video file bytes with a caller-specified filename
/// (extension helps the server pick a decoder) and a caller-specified timeout.
pub fn transcribe_file_bytes(
    bytes: &[u8],
    filename: &str,
    timeout_secs: u64,
    language: &str,
    cfg: &WhisperCfg,
) -> Result<String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(timeout_secs.max(30)))
        .build()?;
    ensure_up(&client, cfg).context("whisper unreachable")?;
    let mut form = multipart::Form::new()
        .part(
            "file",
            multipart::Part::bytes(bytes.to_vec())
                .file_name(filename.to_string())
                .mime_str("application/octet-stream")?,
        )
        .text("model", cfg.model_param.clone())
        .text("response_format", cfg.response_format.clone());
    if !language.is_empty() {
        form = form.text("language", language.to_string());
    }
    let resp = client
        .post(cfg.transcribe_url())
        .multipart(form)
        .send()
        .context("transcribe POST")?;
    if !resp.status().is_success() {
        let st = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(anyhow!("transcribe HTTP {}: {}", st, body));
    }
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = resp.text().context("transcribe read body")?;
    log::info!(
        "transcribe response: ct={:?} body[..500]={:?}",
        ct,
        body.chars().take(500).collect::<String>()
    );
    let text = if ct.contains("application/json") || body.trim_start().starts_with('{') {
        let v: serde_json::Value = serde_json::from_str(&body)
            .with_context(|| format!("transcribe JSON parse; body={body:?}"))?;
        v.get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    } else {
        body.trim().to_string()
    };
    Ok(text)
}

/// Speaker-detection request. Mirrors reef's dropdown.
#[derive(Clone, Copy, Debug)]
pub enum SpeakerMode {
    Off,
    AutoMin(u32),
    Exact(u32),
    /// `pitch=true` — F0-based 2-speaker split, skips pyannote.
    GenderOnly,
}

pub fn speaker_mode_label(m: SpeakerMode) -> String {
    match m {
        SpeakerMode::Off => "Off".into(),
        SpeakerMode::AutoMin(_) => "Auto (2+)".into(),
        SpeakerMode::Exact(n) => format!("Exactly {n}"),
        SpeakerMode::GenderOnly => "Pitch (2 speakers)".into(),
    }
}

pub fn speaker_mode_choices() -> [(SpeakerMode, &'static str); 6] {
    [
        (SpeakerMode::Off, "Off"),
        (SpeakerMode::AutoMin(2), "Auto (2+)"),
        (SpeakerMode::Exact(2), "Exactly 2"),
        (SpeakerMode::Exact(3), "Exactly 3"),
        (SpeakerMode::Exact(4), "Exactly 4"),
        (SpeakerMode::GenderOnly, "Pitch (2 speakers)"),
    ]
}

/// Transcribe a file and request verbose_json. Speaker params per `spk`,
/// language as ISO code ("" = auto-detect).
pub fn transcribe_file_verbose(
    bytes: &[u8],
    filename: &str,
    timeout_secs: u64,
    spk: SpeakerMode,
    language: &str,
    cfg: &WhisperCfg,
) -> Result<TranscribeResult> {
    let client = Client::builder()
        .timeout(Duration::from_secs(timeout_secs.max(30)))
        .build()?;
    ensure_up(&client, cfg).context("whisper unreachable")?;
    let mut form = multipart::Form::new()
        .part(
            "file",
            multipart::Part::bytes(bytes.to_vec())
                .file_name(filename.to_string())
                .mime_str("application/octet-stream")?,
        )
        .text("model", cfg.model_param.clone())
        .text("response_format", "verbose_json");
    if !language.is_empty() {
        form = form.text("language", language.to_string());
    }
    match spk {
        SpeakerMode::Off => {}
        SpeakerMode::AutoMin(n) => {
            form = form.text("diarize", "true").text("min_speakers", n.to_string());
        }
        SpeakerMode::Exact(n) => {
            form = form.text("diarize", "true").text("num_speakers", n.to_string());
        }
        SpeakerMode::GenderOnly => {
            form = form.text("pitch", "true");
        }
    }
    let resp = client
        .post(cfg.transcribe_url())
        .multipart(form)
        .send()
        .context("transcribe POST")?;
    if !resp.status().is_success() {
        let st = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(anyhow!("transcribe HTTP {}: {}", st, body));
    }
    let body = resp.text().context("transcribe read body")?;
    log::info!(
        "transcribe verbose response[..400]={:?}",
        body.chars().take(400).collect::<String>()
    );
    parse_verbose(&body)
}

fn parse_verbose(body: &str) -> Result<TranscribeResult> {
    // Plain-text response: wrap in a single segment (no speaker).
    if !body.trim_start().starts_with('{') {
        let text = body.trim().to_string();
        return Ok(TranscribeResult {
            text: text.clone(),
            duration: None,
            segments: vec![Segment {
                start: 0.0,
                end: 0.0,
                text,
                speaker: None,
            }],
            speaker_count: 0,
        });
    }

    let v: serde_json::Value =
        serde_json::from_str(body).with_context(|| format!("verbose JSON parse; body={body:?}"))?;

    let text = v
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let duration = v.get("duration").and_then(|d| d.as_f64());
    let segments: Vec<Segment> = v
        .get("segments")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|seg| serde_json::from_value::<Segment>(seg.clone()).ok())
                .collect()
        })
        .unwrap_or_default();

    let segments = if segments.is_empty() && !text.is_empty() {
        vec![Segment {
            start: 0.0,
            end: duration.unwrap_or(0.0),
            text: text.clone(),
            speaker: None,
        }]
    } else {
        segments
    };

    let speaker_count = segments
        .iter()
        .filter_map(|s| s.speaker.as_ref())
        .collect::<BTreeSet<_>>()
        .len();

    Ok(TranscribeResult {
        text,
        duration,
        segments,
        speaker_count,
    })
}

fn ensure_up(client: &Client, cfg: &WhisperCfg) -> Result<()> {
    if health_ok(client, &cfg.health_url()) { return Ok(()); }
    log::warn!("whisper health failed, attempting /start");
    let body = serde_json::to_value(&cfg.start_body)?;
    let r = client
        .post(&cfg.start_url)
        .json(&body)
        .timeout(Duration::from_secs(10))
        .send();
    match r {
        Ok(resp) if resp.status().is_success() => {}
        Ok(resp) => return Err(anyhow!("start HTTP {}: {}", resp.status(), resp.text().unwrap_or_default())),
        Err(e) => return Err(anyhow!("start request failed: {e}")),
    }
    for _ in 0..20 {
        if health_ok(client, &cfg.health_url()) { return Ok(()); }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(anyhow!("whisper failed to come up within 10s"))
}

fn health_ok(client: &Client, url: &str) -> bool {
    client
        .get(url)
        .timeout(Duration::from_secs(1))
        .send()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}
