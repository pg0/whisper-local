#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use crossbeam_channel::{unbounded, RecvTimeoutError};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
};
use whisper_local::audio::AudioCapture;
use whisper_local::config::Config;
use whisper_local::hotkey::{spawn_hook, HotkeyEvent};
use whisper_local::overlay::{self, OverlayCmd, OverlayHandle};
use whisper_local::tray::{Tray, TrayEvent};
use whisper_local::{inject, settings_ui, whisper};

#[allow(dead_code)]
enum AppMsg {
    Hotkey(HotkeyEvent),
    Tray(TrayEvent),
    ReloadConfig,
}

fn main() -> anyhow::Result<()> {
    // Child-process modes.
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--settings") {
        init_logging();
        let cfg = Config::load()?;
        let _ = settings_ui::open(cfg);
        return Ok(());
    }
    #[cfg(feature = "transcribe-file")]
    if args.iter().any(|a| a == "--transcribe-file") {
        init_logging();
        let cfg = Config::load()?;
        whisper_local::transcribe_ui::open(cfg);
        return Ok(());
    }
    if args.iter().any(|a| a == "--overlay") {
        init_logging();
        whisper_local::overlay::run_main_thread();
        return Ok(());
    }

    init_logging();
    let cfg = Arc::new(Mutex::new(Config::load()?));
    log::info!(
        "whisper-local starting; whisper base = {}",
        cfg.lock().whisper.base_url
    );

    let overlay = overlay::spawn();
    let mut tray = {
        let c = cfg.lock();
        Tray::new(&c.mic_name, &c.language)?
    };

    let (hk_tx, hk_rx) = unbounded::<HotkeyEvent>();
    spawn_hook(hk_tx)?;

    // Merge hotkey events into one app channel.
    let (app_tx, app_rx) = unbounded::<AppMsg>();
    {
        let tx = app_tx.clone();
        std::thread::spawn(move || {
            while let Ok(ev) = hk_rx.recv() {
                if tx.send(AppMsg::Hotkey(ev)).is_err() {
                    break;
                }
            }
        });
    }

    // Current recording slot.
    let recording: Arc<Mutex<Option<AudioCapture>>> = Arc::new(Mutex::new(None));

    loop {
        // Pump Win32 messages so tray-icon clicks/menu events dispatch.
        pump_win32_messages();

        // Pump tray events (try_recv style) before blocking recv.
        while let Some(ev) = poll_tray_event(&tray) {
            handle_tray_event(ev, &overlay, &cfg, &app_tx)?;
        }

        // Block for up to 100ms on the app channel (so we keep pumping tray).
        match app_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(AppMsg::Hotkey(ev)) => match ev {
                HotkeyEvent::StartRecording => {
                    start_capture(&cfg, &recording, &overlay, false, &mut tray)
                }
                HotkeyEvent::StartLatched => {
                    start_capture(&cfg, &recording, &overlay, true, &mut tray)
                }
                HotkeyEvent::StopAndTranscribe => {
                    stop_and_transcribe(&cfg, &recording, &overlay, &mut tray)
                }
                HotkeyEvent::DiscardRecording => discard(&recording, &overlay, &mut tray),
                HotkeyEvent::MaybeDoubleTapExpired => {}
            },
            Ok(AppMsg::Tray(ev)) => {
                handle_tray_event(ev, &overlay, &cfg, &app_tx)?;
            }
            Ok(AppMsg::ReloadConfig) => {
                *cfg.lock() = Config::load().unwrap_or_else(|e| {
                    log::error!("reload config: {e}");
                    cfg.lock().clone()
                });
                log::info!(
                    "config reloaded; whisper base = {}",
                    cfg.lock().whisper.base_url
                );
            }
            Err(RecvTimeoutError::Timeout) => {} // continue, pump tray
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}

/// Handle a TrayEvent: open settings, quit, or pick a mic.
fn handle_tray_event(
    ev: TrayEvent,
    overlay: &OverlayHandle,
    cfg: &Arc<Mutex<Config>>,
    app_tx: &crossbeam_channel::Sender<AppMsg>,
) -> anyhow::Result<()> {
    match ev {
        TrayEvent::OpenSettings => {
            let exe = std::env::current_exe()?;
            let app_tx2 = app_tx.clone();
            std::thread::spawn(move || {
                let mut child =
                    match std::process::Command::new(&exe).arg("--settings").spawn() {
                        Ok(c) => c,
                        Err(e) => {
                            log::error!("spawn settings: {e}");
                            return;
                        }
                    };
                let _ = child.wait();
                let _ = app_tx2.send(AppMsg::ReloadConfig);
            });
        }
        TrayEvent::Quit => {
            overlay.quit();
            std::process::exit(0);
        }
        TrayEvent::SelectMic(name) => {
            let mut c = cfg.lock();
            c.mic_name = name.clone();
            if let Err(e) = c.save() {
                log::error!("save config after mic change: {e}");
            }
            log::info!(
                "mic selected: {}",
                if name.is_empty() { "(default)" } else { &name }
            );
        }
        TrayEvent::SelectLanguage(code) => {
            let mut c = cfg.lock();
            c.language = code.clone();
            if let Err(e) = c.save() {
                log::error!("save config after language change: {e}");
            }
            log::info!(
                "language selected: {}",
                if code.is_empty() { "(auto)" } else { &code }
            );
        }
        #[cfg(feature = "transcribe-file")]
        TrayEvent::OpenTranscribeFile => {
            let exe = std::env::current_exe()?;
            std::thread::spawn(move || {
                match std::process::Command::new(&exe)
                    .arg("--transcribe-file")
                    .spawn()
                {
                    Ok(mut child) => {
                        let _ = child.wait();
                    }
                    Err(e) => log::error!("spawn transcribe-file: {e}"),
                }
            });
        }
    }
    Ok(())
}

/// Poll for the next tray event using the try_recv API exposed by Tray.
fn poll_tray_event(tray: &Tray) -> Option<TrayEvent> {
    tray.try_recv()
}

/// Drain any pending Win32 messages for this thread. Required so the
/// tray-icon hidden window receives and dispatches shell notifications
/// (right-click menu, icon clicks, menu commands).
fn pump_win32_messages() {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn start_capture(
    cfg: &Arc<Mutex<Config>>,
    slot: &Arc<Mutex<Option<AudioCapture>>>,
    overlay: &OverlayHandle,
    latched: bool,
    tray: &mut Tray,
) {
    let mic = cfg.lock().mic_name.clone();
    log::info!(
        "start_capture: mic={}",
        if mic.is_empty() { "(default)" } else { &mic }
    );
    match AudioCapture::start(&mic) {
        Ok(cap) => {
            let rms_rx = cap.rms_rx.clone();
            let ov_tx = overlay.0.clone();
            std::thread::spawn(move || {
                let mut count = 0u32;
                let mut max_rms: f32 = 0.0;
                let mut sum_rms: f32 = 0.0;
                while let Ok(r) = rms_rx.recv() {
                    count += 1;
                    if r > max_rms { max_rms = r; }
                    sum_rms += r;
                    let _ = ov_tx.send(OverlayCmd::PushRms(r));
                }
                let mean = if count > 0 { sum_rms / count as f32 } else { 0.0 };
                log::info!(
                    "rms stats: chunks={} max={:.4} mean={:.4}",
                    count, max_rms, mean
                );
            });
            *slot.lock() = Some(cap);
            if latched {
                overlay.show_latched();
            } else {
                overlay.show_recording();
            }
            tray.set_active(true);
        }
        Err(e) => {
            log::error!("mic start failed: {e}");
            overlay.show_error("Mic unavailable".into());
        }
    }
}

fn stop_and_transcribe(
    cfg: &Arc<Mutex<Config>>,
    slot: &Arc<Mutex<Option<AudioCapture>>>,
    overlay: &OverlayHandle,
    tray: &mut Tray,
) {
    let cap = slot.lock().take();
    let Some(cap) = cap else {
        return;
    };
    overlay.hide();
    tray.set_active(false);
    let (whisper_cfg, language) = {
        let c = cfg.lock();
        (c.whisper.clone(), c.language.clone())
    };
    let overlay_clone = overlay.clone();
    // Stop the stream on the current thread (cpal::Stream is !Send).
    // Then hand the WAV bytes to a background thread for the network call.
    match cap.stop() {
        Err(e) => {
            log::error!("audio stop: {e}");
            overlay_clone.show_error("Audio error".into());
        }
        Ok(wav) => {
            log::info!("captured wav: {} bytes", wav.len());
            if let Err(e) = dump_wav_to_disk(&wav) {
                log::warn!("dump last.wav: {e}");
            }
            std::thread::spawn(move || {
                match whisper::transcribe(&wav, &language, &whisper_cfg) {
                    Ok(text) if !text.is_empty() => {
                        log::info!("transcript: {}", text);
                        inject::type_text(&text);
                    }
                    Ok(_) => log::info!("empty transcript"),
                    Err(e) => {
                        log::error!("whisper: {e}");
                        overlay_clone.show_error(short_err(&e));
                    }
                }
            });
        }
    }
}

fn discard(
    slot: &Arc<Mutex<Option<AudioCapture>>>,
    overlay: &OverlayHandle,
    tray: &mut Tray,
) {
    slot.lock().take();
    overlay.hide();
    tray.set_active(false);
}

/// Write the captured WAV to `%APPDATA%\whisper-local\last.wav` and a timestamped
/// copy in `%APPDATA%\whisper-local\debug\<ts>.wav` so the user can replay it.
fn dump_wav_to_disk(wav: &[u8]) -> anyhow::Result<()> {
    let dir = whisper_local::config::config_dir()?;
    std::fs::create_dir_all(&dir)?;
    let last = dir.join("last.wav");
    std::fs::write(&last, wav)?;
    let debug_dir = dir.join("debug");
    let _ = std::fs::create_dir_all(&debug_dir);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = std::fs::write(debug_dir.join(format!("{ts}.wav")), wav);
    log::info!("wrote {}", last.display());
    Ok(())
}

fn short_err(e: &anyhow::Error) -> String {
    let s = format!("{e}");
    if s.contains("unreachable") || s.contains("dns") || s.contains("refused") {
        "Whisper unreachable".into()
    } else if s.contains("failed to come up") {
        "Whisper start failed".into()
    } else {
        "Transcribe failed".into()
    }
}

fn init_logging() {
    use std::fs::OpenOptions;
    let dir = whisper_local::config::config_dir().unwrap_or_else(|_| std::env::temp_dir());
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("log.txt");
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > 1_048_576 {
            let _ = std::fs::rename(&path, dir.join("log.txt.1"));
        }
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();
    env_logger::Builder::new()
        .parse_env(env_logger::Env::default().default_filter_or(
            if std::env::var("WHISPER_DEBUG").is_ok() {
                "debug"
            } else {
                "info"
            },
        ))
        .target(match file {
            Some(f) => env_logger::Target::Pipe(Box::new(f)),
            None => env_logger::Target::Stderr,
        })
        .init();
}
