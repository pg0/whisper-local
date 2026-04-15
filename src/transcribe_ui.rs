//! Drag-and-drop file-transcription window.
//!
//! Launched as a child process with `--transcribe-file`. User drops an audio
//! or video file onto the window; we POST the file bytes to the whisper
//! server and show the transcript with Copy / Save-as buttons.

use crate::config::Config;
use crate::whisper::{self, Segment, SpeakerMode};
use anyhow::Result;
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::{egui, NativeOptions};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone)]
enum State {
    Idle,
    Working {
        filename: String,
        started: Instant,
    },
    Done {
        filename: String,
        text: String,
        segments: Vec<Segment>,
        speaker_count: usize,
        /// Distinct speaker labels in first-seen order, precomputed once.
        #[cfg(feature = "speaker-detection")]
        speakers: Vec<String>,
        duration_secs: u64,
        words: usize,
        show_speakers: bool,
    },
    Error(String),
}

struct Shared {
    state: Mutex<State>,
    speaker_mode: Mutex<SpeakerMode>,
    /// Per-session language override. Empty = auto-detect, otherwise ISO code
    /// forwarded as `language` form field. Starts from Config.language.
    language: Mutex<String>,
    result_tx: Sender<WorkResult>,
    result_rx: Receiver<WorkResult>,
}

fn language_label(code: &str) -> &'static str {
    crate::config::LANGUAGES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, l)| *l)
        .unwrap_or("Auto-detect")
}

enum WorkResult {
    Ok {
        filename: String,
        text: String,
        segments: Vec<Segment>,
        speaker_count: usize,
        duration_secs: u64,
    },
    Err(String),
}

const WIN_TITLE: &str = "whisper-local — Transcribe file";
const WIN_SIZE: [f32; 2] = [420.0, 320.0];

pub fn open(cfg: Config) {
    // Single-instance: if another transcribe window is already running,
    // bring it to the foreground and bail instead of opening a second copy.
    let _guard = match acquire_single_instance() {
        Some(g) => g,
        None => {
            focus_existing_window();
            return;
        }
    };

    let (tx, rx) = unbounded::<WorkResult>();
    let shared = Arc::new(Shared {
        state: Mutex::new(State::Idle),
        speaker_mode: Mutex::new(SpeakerMode::Off),
        language: Mutex::new(cfg.language.clone()),
        result_tx: tx,
        result_rx: rx,
    });

    let mut vb = egui::ViewportBuilder::default()
        .with_title(WIN_TITLE)
        .with_inner_size(WIN_SIZE)
        .with_resizable(false)
        .with_maximize_button(false);
    if let Some(ic) = crate::app_icon::icon_data() {
        vb = vb.with_icon(ic);
    }
    let opts = NativeOptions { viewport: vb, ..Default::default() };

    let shared_for_ui = shared.clone();
    let _ = eframe::run_native(
        "whisper-local-transcribe-file",
        opts,
        Box::new(move |cc| {
            crate::fonts::install_broad_unicode_font(&cc.egui_ctx);
            Box::new(App {
                shared: shared_for_ui,
                cfg,
            })
        }),
    );
}

/// Windows named-mutex guard used to enforce single-instance.
struct InstanceGuard(windows::Win32::Foundation::HANDLE);
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        use windows::Win32::Foundation::CloseHandle;
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn acquire_single_instance() -> Option<InstanceGuard> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;
    unsafe {
        let name = HSTRING::from("Local\\whisper-local-transcribe-file-v1");
        let h = match CreateMutexW(None, true, PCWSTR(name.as_ptr())) {
            Ok(h) => h,
            Err(_) => return None,
        };
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(h);
            return None;
        }
        Some(InstanceGuard(h))
    }
}

fn focus_existing_window() {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    unsafe {
        let title = HSTRING::from(WIN_TITLE);
        let hwnd = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr()));
        if hwnd.0 != 0 {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

struct App {
    shared: Arc<Shared>,
    cfg: Config,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Pick up worker results.
        while let Ok(res) = self.shared.result_rx.try_recv() {
            let mut st = self.shared.state.lock();
            *st = match res {
                WorkResult::Ok {
                    filename,
                    text,
                    segments,
                    speaker_count,
                    duration_secs,
                } => {
                    let words = text.split_whitespace().count();
                    let diarize_requested = !matches!(
                        *self.shared.speaker_mode.lock(),
                        SpeakerMode::Off
                    );
                    let show_speakers = diarize_requested && speaker_count > 1;
                    #[cfg(feature = "speaker-detection")]
                    let speakers = distinct_speakers(&segments);
                    State::Done {
                        filename,
                        text,
                        segments,
                        speaker_count,
                        #[cfg(feature = "speaker-detection")]
                        speakers,
                        duration_secs,
                        words,
                        show_speakers,
                    }
                }
                WorkResult::Err(msg) => State::Error(msg),
            };
        }

        // Accept dropped files.
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if !dropped.is_empty() {
            if let Some(path) = dropped.iter().find_map(|f| f.path.clone()) {
                self.start_transcribe(path);
            }
        }

        let state = self.shared.state.lock().clone();
        ctx.request_repaint_after(Duration::from_millis(200));

        egui::CentralPanel::default().show(ctx, |ui| {
            let idle = matches!(state, State::Idle);
            ui.horizontal(|ui| {
                ui.label("Language:");
                let current = self.shared.language.lock().clone();
                ui.add_enabled_ui(idle, |ui| {
                    egui::ComboBox::from_id_source("language")
                        .selected_text(language_label(&current))
                        .show_ui(ui, |ui| {
                            for (code, label) in crate::config::LANGUAGES {
                                let sel = current == *code;
                                if ui.selectable_label(sel, *label).clicked() {
                                    *self.shared.language.lock() = (*code).to_string();
                                }
                            }
                        });
                });

                #[cfg(feature = "speaker-detection")]
                if self.cfg.enable_speaker_detection {
                    ui.add_space(12.0);
                    ui.label("Speakers:");
                    let current = *self.shared.speaker_mode.lock();
                    ui.add_enabled_ui(idle, |ui| {
                        egui::ComboBox::from_id_source("speaker_mode")
                            .selected_text(whisper::speaker_mode_label(current))
                            .show_ui(ui, |ui| {
                                for (mode, label) in whisper::speaker_mode_choices() {
                                    let sel = whisper::speaker_mode_label(current) == label;
                                    if ui.selectable_label(sel, label).clicked() {
                                        *self.shared.speaker_mode.lock() = mode;
                                    }
                                }
                            });
                    })
                    .response
                    .on_hover_text("diarize + min_speakers/num_speakers");
                }
            });
            #[cfg(feature = "speaker-detection")]
            if !self.cfg.enable_speaker_detection
                && !matches!(*self.shared.speaker_mode.lock(), SpeakerMode::Off)
            {
                *self.shared.speaker_mode.lock() = SpeakerMode::Off;
            }
            ui.add_space(6.0);

            ui.vertical_centered(|ui| {
                match state {
                    State::Idle => {
                        let zone_h = (ui.available_height() - 10.0).max(130.0);
                        if draw_drop_zone_sized(ui, "Drop audio or video here  ·  or click to browse", zone_h).clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter(
                                    "Media",
                                    &[
                                        "mp3", "wav", "m4a", "flac", "ogg", "opus", "webm",
                                        "mp4", "mkv", "mov", "avi", "aac",
                                    ],
                                )
                                .add_filter("All files", &["*"])
                                .pick_file()
                            {
                                self.start_transcribe(path);
                            }
                        }
                    }
                    State::Working { filename, started } => {
                        let zone_h = (ui.available_height() - 40.0).max(130.0);
                        draw_drop_zone_sized(ui, &ellipsize(&filename, 60), zone_h);
                        ui.add_space(8.0);
                        let elapsed = started.elapsed();
                        let stage = working_stage(elapsed);
                        ui.label(
                            egui::RichText::new(format!(
                                "elapsed {}s  ·  {stage}",
                                elapsed.as_secs()
                            ))
                            .size(13.0),
                        );
                    }
                    State::Done {
                        filename,
                        text,
                        segments,
                        speaker_count,
                        #[cfg(feature = "speaker-detection")]
                        speakers,
                        duration_secs,
                        words,
                        show_speakers,
                    } => {
                        ui.label(egui::RichText::new(&filename).size(13.0));
                        ui.add_space(4.0);
                        let speaker_line = if speaker_count > 1 {
                            format!(" · {speaker_count} speakers")
                        } else {
                            String::new()
                        };
                        ui.label(
                            egui::RichText::new(format!(
                                "{} transcribed · {words} words{speaker_line}",
                                fmt_duration(duration_secs)
                            ))
                            .strong()
                            .size(14.0),
                        );
                        ui.add_space(8.0);

                        let scroll_h = (ui.available_height() - 44.0).max(120.0);
                        egui::ScrollArea::vertical()
                            .max_height(scroll_h)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                #[cfg(feature = "speaker-detection")]
                                let use_layout = show_speakers;
                                #[cfg(not(feature = "speaker-detection"))]
                                let use_layout = false;
                                if use_layout {
                                    #[cfg(feature = "speaker-detection")]
                                    {
                                        let job = build_speaker_layout(&segments);
                                        ui.add(egui::Label::new(job).wrap(true));
                                    }
                                } else {
                                    let mut shown = text.clone();
                                    ui.add_sized(
                                        [ui.available_width(), ui.available_height()],
                                        egui::TextEdit::multiline(&mut shown)
                                            .desired_width(f32::INFINITY)
                                            .interactive(true),
                                    );
                                }
                            });
                        ui.add_space(8.0);

                        ui.horizontal(|ui| {
                            if ui.button("New file").clicked() {
                                *self.shared.state.lock() = State::Idle;
                            }

                            // Show-speakers toggle: only compiled in with the
                            // speaker-detection feature.
                            #[cfg(feature = "speaker-detection")]
                            {
                                let has_speakers = speaker_count > 1;
                                let label = if show_speakers {
                                    "👥 Hide speakers"
                                } else {
                                    "👥 Show speakers"
                                };
                                let resp = ui.add_enabled(has_speakers, egui::Button::new(label));
                                if !has_speakers {
                                    resp.on_hover_text("Only one speaker detected");
                                } else if resp.clicked() {
                                    if let State::Done { show_speakers, .. } =
                                        &mut *self.shared.state.lock()
                                    {
                                        *show_speakers = !*show_speakers;
                                    }
                                }
                            }

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    #[cfg(feature = "speaker-detection")]
                                    {
                                        ui.menu_button("💾  Save…", |ui| {
                                            if ui.button("text only [.txt]").clicked() {
                                                let out = render_text(&text, &segments, show_speakers);
                                                save_txt(&filename, &out, false);
                                                ui.close_menu();
                                            }
                                            if ui.button("timestamped [.txt]").clicked() {
                                                let out = render_timestamped(&segments);
                                                save_txt(&filename, &out, true);
                                                ui.close_menu();
                                            }
                                            if speakers.len() > 1 {
                                                ui.separator();
                                                for spk in &speakers {
                                                    if ui.button(format!("only {spk} [.txt]")).clicked() {
                                                        let out = render_only_speaker(&segments, spk);
                                                        save_txt_suffixed(
                                                            &filename,
                                                            &out,
                                                            Some(&sanitize_speaker(spk)),
                                                        );
                                                        ui.close_menu();
                                                    }
                                                }
                                            }
                                        });
                                        if speakers.len() > 1 {
                                            ui.menu_button("📋  Clipboard", |ui| {
                                                if ui.button("all speakers").clicked() {
                                                    copy_to_clipboard(render_text(
                                                        &text, &segments, show_speakers,
                                                    ));
                                                    ui.close_menu();
                                                }
                                                ui.separator();
                                                for spk in &speakers {
                                                    if ui.button(format!("only {spk}")).clicked() {
                                                        copy_to_clipboard(render_only_speaker(
                                                            &segments, spk,
                                                        ));
                                                        ui.close_menu();
                                                    }
                                                }
                                            });
                                        } else if ui.button("📋  Clipboard").clicked() {
                                            copy_to_clipboard(render_text(
                                                &text, &segments, show_speakers,
                                            ));
                                        }
                                    }

                                    #[cfg(not(feature = "speaker-detection"))]
                                    {
                                        if ui.button("💾  Save [.txt]").clicked() {
                                            save_txt(&filename, &text, false);
                                        }
                                        if ui.button("📋  Clipboard").clicked() {
                                            copy_to_clipboard(text.clone());
                                        }
                                    }
                                },
                            );
                        });
                    }
                    State::Error(msg) => {
                        ui.colored_label(
                            egui::Color32::from_rgb(230, 90, 90),
                            format!("⚠  {msg}"),
                        );
                        ui.add_space(6.0);
                        if ui.button("Try again").clicked() {
                            *self.shared.state.lock() = State::Idle;
                        }
                    }
                }
            });
        });
    }
}

fn copy_to_clipboard(text: impl Into<String>) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set_text(text.into());
    }
}

fn draw_drop_zone_sized(ui: &mut egui::Ui, label: &str, height: f32) -> egui::Response {
    let desired = egui::vec2(ui.available_width().min(380.0), height);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());
    let hovered = resp.hovered();
    let bg = if hovered {
        egui::Color32::from_rgb(244, 244, 248)
    } else {
        egui::Color32::from_rgb(236, 236, 240)
    };
    let stroke_color = if hovered {
        egui::Color32::from_rgb(120, 120, 130)
    } else {
        egui::Color32::from_rgb(170, 170, 180)
    };
    let painter = ui.painter_at(rect);
    painter.rect(rect, 12.0, bg, egui::Stroke::new(1.2, stroke_color));
    painter.text(
        rect.center() + egui::vec2(0.0, -22.0),
        egui::Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(44.0),
        egui::Color32::from_rgb(60, 60, 70),
    );
    painter.text(
        rect.center() + egui::vec2(0.0, 30.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(13.0),
        egui::Color32::from_rgb(40, 40, 50),
    );
    resp
}

/// Truncate a string to at most `max` visible characters, adding an ellipsis.
fn ellipsize(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}

/// Pick a user-facing status label for the current elapsed time.
/// Stages are synthetic (the server does the real work end-to-end), but they
/// match what happens conceptually and give the user a sense of progress.
fn working_stage(elapsed: Duration) -> &'static str {
    let ms = elapsed.as_millis();
    if ms < 600 {
        "Reading file…"
    } else if ms < 1800 {
        "Extracting audio…"
    } else if ms < 3500 {
        "Converting audio…"
    } else {
        "Transcribing…"
    }
}

fn fmt_duration(s: u64) -> String {
    let m = s / 60;
    let r = s % 60;
    if m > 0 {
        format!("{m}m {r:02}s")
    } else {
        format!("{r}s")
    }
}

#[cfg(feature = "speaker-detection")]
fn speaker_color(speaker: &str) -> egui::Color32 {
    const PALETTE: &[(u8, u8, u8)] = &[
        (40, 110, 190),   // blue
        (200, 70, 70),    // red
        (50, 140, 80),    // green
        (190, 110, 40),   // orange
        (140, 70, 180),   // purple
        (30, 130, 140),   // teal
        (180, 60, 130),   // magenta
        (110, 110, 40),   // olive
    ];
    let mut h: u32 = 2166136261;
    for b in speaker.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    let (r, g, b) = PALETTE[(h as usize) % PALETTE.len()];
    egui::Color32::from_rgb(r, g, b)
}

#[cfg(feature = "speaker-detection")]
fn build_speaker_layout(segments: &[Segment]) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let body_fmt = egui::TextFormat {
        color: egui::Color32::from_rgb(30, 30, 35),
        font_id: egui::FontId::proportional(13.0),
        ..Default::default()
    };
    let mut current: Option<String> = None;
    for seg in segments {
        let spk = seg.speaker.as_deref().unwrap_or("");
        if !spk.is_empty() && current.as_deref() != Some(spk) {
            if !job.text.is_empty() {
                job.append("\n\n", 0.0, egui::TextFormat::default());
            }
            let color = speaker_color(spk);
            let label_fmt = egui::TextFormat {
                color,
                font_id: egui::FontId::proportional(13.0),
                ..Default::default()
            };
            job.append(&format!("[{spk}]  "), 0.0, label_fmt);
            current = Some(spk.to_string());
        }
        job.append(&format!("{} ", seg.text.trim()), 0.0, body_fmt.clone());
    }
    job
}

/// Distinct speaker labels in appearance order (empty labels skipped).
#[cfg(feature = "speaker-detection")]
fn distinct_speakers(segments: &[Segment]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for seg in segments {
        if let Some(s) = seg.speaker.as_deref() {
            if !s.is_empty() && !out.iter().any(|x| x == s) {
                out.push(s.to_string());
            }
        }
    }
    out
}

/// Plain text for one speaker, segments joined by space.
#[cfg(feature = "speaker-detection")]
fn render_only_speaker(segments: &[Segment], speaker: &str) -> String {
    let mut out = String::new();
    for seg in segments {
        if seg.speaker.as_deref() == Some(speaker) {
            if !out.is_empty() { out.push(' '); }
            out.push_str(seg.text.trim());
        }
    }
    out
}

/// Safe suffix for filenames — replaces anything non-alphanumeric/underscore/dash.
#[cfg(feature = "speaker-detection")]
fn sanitize_speaker(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

#[cfg(feature = "speaker-detection")]
fn render_text(plain: &str, segments: &[Segment], show_speakers: bool) -> String {
    if !show_speakers {
        return plain.to_string();
    }
    let mut out = String::new();
    let mut current_speaker: Option<&str> = None;
    for seg in segments {
        let spk = seg.speaker.as_deref().unwrap_or("");
        if !spk.is_empty() && Some(spk) != current_speaker {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("[{spk}]\n"));
            current_speaker = Some(spk);
        }
        out.push_str(seg.text.trim());
        out.push(' ');
    }
    out.trim_end().to_string()
}

#[cfg(feature = "speaker-detection")]
fn render_timestamped(segments: &[Segment]) -> String {
    let mut out = String::new();
    for seg in segments {
        let ts = format_ts(seg.start);
        if let Some(spk) = seg.speaker.as_deref() {
            if !spk.is_empty() {
                out.push_str(&format!("[{ts}] [{spk}] {}\n", seg.text.trim()));
                continue;
            }
        }
        out.push_str(&format!("[{ts}] {}\n", seg.text.trim()));
    }
    out
}

#[cfg(feature = "speaker-detection")]
fn format_ts(secs: f64) -> String {
    let s = secs as u64;
    let m = s / 60;
    let r = s % 60;
    format!("{m:02}:{r:02}")
}

fn save_txt(filename: &str, content: &str, timestamped: bool) {
    save_txt_suffixed(
        filename,
        content,
        if timestamped { Some("timestamped") } else { None },
    );
}

fn save_txt_suffixed(filename: &str, content: &str, suffix: Option<&str>) {
    let stem = std::path::Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("transcript");
    let default_name = match suffix {
        Some(s) => format!("{stem}.{s}.txt"),
        None => format!("{stem}.txt"),
    };
    if let Some(path) = rfd::FileDialog::new()
        .set_file_name(default_name)
        .add_filter("Text", &["txt"])
        .save_file()
    {
        let _ = std::fs::write(path, content);
    }
}

impl App {
    fn start_transcribe(&self, path: PathBuf) {
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string();
        {
            let mut st = self.shared.state.lock();
            *st = State::Working {
                filename: filename.clone(),
                started: Instant::now(),
            };
        }
        let tx = self.shared.result_tx.clone();
        let cfg = self.cfg.whisper.clone();
        let language = self.shared.language.lock().clone();
        let spk = *self.shared.speaker_mode.lock();
        std::thread::spawn(move || {
            let res = run_transcribe(&path, &filename, spk, &language, &cfg);
            let _ = tx.send(match res {
                Ok((result, duration_secs)) => WorkResult::Ok {
                    filename,
                    text: result.text,
                    segments: result.segments,
                    speaker_count: result.speaker_count,
                    duration_secs,
                },
                Err(e) => WorkResult::Err(format!("{e:#}")),
            });
        });
    }
}

fn run_transcribe(
    path: &std::path::Path,
    filename: &str,
    spk: SpeakerMode,
    language: &str,
    cfg: &crate::config::WhisperCfg,
) -> Result<(whisper::TranscribeResult, u64)> {
    let bytes = std::fs::read(path)?;
    let start = Instant::now();
    let result =
        whisper::transcribe_file_verbose(&bytes, filename, 30 * 60, spk, language, cfg)?;
    let elapsed = start.elapsed().as_secs();
    let duration = probe_media_duration(path)
        .or_else(|| result.duration.map(|d| d.round() as u64))
        .unwrap_or(elapsed);
    Ok((result, duration))
}

fn probe_media_duration(path: &std::path::Path) -> Option<u64> {
    let out = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let secs: f64 = s.trim().parse().ok()?;
    Some(secs.round() as u64)
}
