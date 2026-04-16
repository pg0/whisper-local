#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use crossbeam_channel::{unbounded, RecvTimeoutError};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
};
use whisper_local::audio::AudioCapture;
use whisper_local::config::Config;
use whisper_local::hotkey::{force_idle, force_latch, spawn_hook, HotkeyEvent};
use whisper_local::overlay::{self, OverlayCmd, OverlayHandle};
use whisper_local::postprocess::{self, Action};
use whisper_local::tray::{Tray, TrayEvent};
use whisper_local::{inject, whisper};
#[cfg(feature = "gui")]
use whisper_local::settings_ui;

#[allow(dead_code)]
enum AppMsg {
    Hotkey(HotkeyEvent),
    Tray(TrayEvent),
    ReloadConfig,
    /// Continuous hands-free: transcript has been typed, restart recording.
    RestartContinuous,
}

fn main() -> anyhow::Result<()> {
    // Child-process modes.
    let args: Vec<String> = std::env::args().collect();
    #[cfg(feature = "gui")]
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
    #[cfg(feature = "overlay-ui")]
    if args.iter().any(|a| a == "--overlay") {
        init_logging();
        whisper_local::overlay::run_main_thread();
        return Ok(());
    }

    init_logging();
    bind_children_to_job();
    let cfg = Arc::new(Mutex::new(Config::load()?));
    log::info!(
        "whisper-local starting; whisper base = {}",
        cfg.lock().whisper.base_url
    );
    bootstrap_replace_maps();

    let overlay = overlay::spawn();
    let mut tray = {
        let c = cfg.lock();
        let all = postprocess::list_replace_maps();
        Tray::new(
            &c.mic_name,
            &c.language,
            c.newline_feed,
            c.command_mode,
            c.replace_maps_enabled,
            &all,
            &c.enabled_replace_maps,
        )?
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
    // Set by the watcher when auto-stop fires so the main loop knows this
    // StopAndTranscribe came from silence detection (not a user chord press).
    let auto_stop_pending = Arc::new(AtomicBool::new(false));
    // Tracks the listen-mode flags we paused when the user pressed the chord
    // mid-listen, so we can restore them after the dictation finishes.
    let suspended_listen: Arc<Mutex<Option<(bool, bool)>>> = Arc::new(Mutex::new(None));
    // Pre-listen state of command_mode, restored when listen mode toggles off.
    let listen_saved_cmd: Arc<Mutex<Option<bool>>> = Arc::new(Mutex::new(None));
    let map_cache: Arc<Mutex<postprocess::MapCache>> = Arc::new(Mutex::new(Default::default()));

    loop {
        // Pump Win32 messages so tray-icon clicks/menu events dispatch.
        pump_win32_messages();

        // Pump tray events (try_recv style) before blocking recv.
        while let Some(ev) = poll_tray_event(&tray) {
            if matches!(ev, TrayEvent::ToggleListen) {
                handle_toggle_listen(
                    &cfg,
                    &recording,
                    &overlay,
                    &mut tray,
                    &app_tx,
                    &auto_stop_pending,
                    &listen_saved_cmd,
                );
            } else {
                handle_tray_event(ev, &overlay, &cfg, &app_tx)?;
            }
        }

        // Block for up to 100ms on the app channel (so we keep pumping tray).
        match app_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(AppMsg::Hotkey(ev)) => match ev {
                HotkeyEvent::StartRecording | HotkeyEvent::StartLatched => {
                    // Pressing the chord while in tray-toggled "listen" mode
                    // pauses it for this dictation; restored after the chunk
                    // is typed (see StopAndTranscribe handler).
                    let mut c = cfg.lock();
                    if c.command_mode || c.continuous {
                        *suspended_listen.lock() = Some((c.continuous, c.command_mode));
                        c.command_mode = false;
                        c.continuous = false;
                        let _ = c.save();
                        tray.set_command_mode(false);
                        log::info!("chord pressed -> listen suspended");
                    }
                    drop(c);
                    let latched = matches!(ev, HotkeyEvent::StartLatched);
                    start_capture(
                        &cfg,
                        &recording,
                        &overlay,
                        latched,
                        &mut tray,
                        &app_tx,
                        &auto_stop_pending,
                    );
                }
                HotkeyEvent::StopAndTranscribe => {
                    let was_auto = auto_stop_pending.swap(false, Ordering::SeqCst);
                    stop_and_transcribe(
                        &cfg, &recording, &overlay, &mut tray, &app_tx, was_auto,
                        &map_cache,
                    );
                    // Restore listen mode if the chord had paused it.
                    if let Some((cont, cmd)) = suspended_listen.lock().take() {
                        let mut c = cfg.lock();
                        c.continuous = cont;
                        c.command_mode = cmd;
                        let _ = c.save();
                        tray.set_command_mode(cmd);
                        log::info!("listen restored after dictation");
                    }
                }
                HotkeyEvent::DiscardRecording => discard(&recording, &overlay, &mut tray),
                HotkeyEvent::MaybeDoubleTapExpired => {}
            },
            Ok(AppMsg::Tray(TrayEvent::ToggleListen)) => {
                handle_toggle_listen(
                    &cfg,
                    &recording,
                    &overlay,
                    &mut tray,
                    &app_tx,
                    &auto_stop_pending,
                    &listen_saved_cmd,
                );
            }
            Ok(AppMsg::Tray(ev)) => {
                handle_tray_event(ev, &overlay, &cfg, &app_tx)?;
            }
            Ok(AppMsg::RestartContinuous) => {
                log::info!("continuous: restarting recording after transcript");
                start_capture(
                    &cfg,
                    &recording,
                    &overlay,
                    true, // immediately latched — we're mid-loop
                    &mut tray,
                    &app_tx,
                    &auto_stop_pending,
                );
                force_latch();
            }
            Ok(AppMsg::ReloadConfig) => {
                *cfg.lock() = Config::load().unwrap_or_else(|e| {
                    log::error!("reload config: {e}");
                    cfg.lock().clone()
                });
                let c = cfg.lock();
                tray.set_newline_feed(c.newline_feed);
                tray.set_command_mode(c.command_mode);
                tray.set_replace_maps_enabled(c.replace_maps_enabled);
                log::info!(
                    "config reloaded; whisper base = {}",
                    c.whisper.base_url
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
        #[cfg(feature = "gui")]
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
        TrayEvent::ToggleNewlineFeed(enabled) => {
            let mut c = cfg.lock();
            c.newline_feed = enabled;
            if let Err(e) = c.save() {
                log::error!("save config after newline-feed toggle: {e}");
            }
            log::info!("newline-feed: {}", if enabled { "on" } else { "off" });
        }
        TrayEvent::ToggleCommandMode(enabled) => {
            let mut c = cfg.lock();
            c.command_mode = enabled;
            if let Err(e) = c.save() {
                log::error!("save config after command-mode toggle: {e}");
            }
            log::info!("command-mode: {}", if enabled { "on" } else { "off" });
        }
        TrayEvent::ToggleListen => {
            // The main loop has direct access to the recording slot + tray
            // so it handles this event itself. This arm is unreachable now,
            // but kept to keep the match exhaustive.
            log::warn!("ToggleListen reached handle_tray_event (should be handled in main)");
        }
        TrayEvent::ToggleReplaceMaps(enabled) => {
            let mut c = cfg.lock();
            c.replace_maps_enabled = enabled;
            if let Err(e) = c.save() {
                log::error!("save config after replace-maps toggle: {e}");
            }
            log::info!("replace-maps: {}", if enabled { "on" } else { "off" });
        }
        TrayEvent::OpenReplaceMapsFolder => {
            if let Some(dir) = postprocess::replace_maps_dir() {
                let p = dir.clone();
                std::thread::spawn(move || {
                    let _ = std::process::Command::new("explorer.exe")
                        .arg(&p.display().to_string())
                        .spawn();
                });
            }
        }
        TrayEvent::ToggleReplaceMapFile(name, enabled) => {
            let mut c = cfg.lock();
            c.enabled_replace_maps.retain(|n| n != &name);
            if enabled {
                c.enabled_replace_maps.push(name.clone());
            }
            if let Err(e) = c.save() {
                log::error!("save config after replace-map toggle: {e}");
            }
            log::info!(
                "replace-map {}: {}",
                name,
                if enabled { "on" } else { "off" }
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
    app_tx: &crossbeam_channel::Sender<AppMsg>,
    auto_stop_pending: &Arc<AtomicBool>,
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
            let overlay_hf = overlay.clone();
            let app_tx_hf = app_tx.clone();
            let auto_stop_pending_w = auto_stop_pending.clone();
            let (hold_enabled, hold_secs, loop_on, stop_on, stop_secs, silence_thresh) = {
                let c = cfg.lock();
                (
                    c.auto_hold,
                    c.auto_hold_secs,
                    c.continuous,
                    c.auto_stop,
                    c.stop_silence_secs,
                    c.silence_rms_threshold,
                )
            };
            // Loop uses a fixed silence-window tuned for realtime feel
            // without fragmenting mid-sentence pauses.
            const LOOP_SILENCE_SECS: f32 = 0.6;
            let silence_secs = if loop_on { LOOP_SILENCE_SECS } else { stop_secs };
            let silence_stops = loop_on || stop_on;
            std::thread::spawn(move || {
                let mut count = 0u32;
                let mut max_rms: f32 = 0.0;
                let mut sum_rms: f32 = 0.0;
                let start = std::time::Instant::now();
                let mut last_loud = start;
                let mut auto_latched = latched;
                let mut auto_stopped = false;
                let mut had_content = false;
                while let Ok(r) = rms_rx.recv() {
                    count += 1;
                    if r > max_rms { max_rms = r; }
                    sum_rms += r;
                    let _ = ov_tx.send(OverlayCmd::PushRms(r));
                    if !auto_stopped {
                        let now = std::time::Instant::now();
                        // Auto-hold: opt-in.
                        if hold_enabled
                            && !auto_latched
                            && now.duration_since(start).as_secs_f32() >= hold_secs
                        {
                            auto_latched = true;
                            force_latch();
                            overlay_hf.show_latched();
                            log::info!("auto-hold: kept recording after {hold_secs}s");
                        }
                        // Content vs silence.
                        if r >= silence_thresh {
                            had_content = true;
                            last_loud = now;
                        } else if silence_stops
                            && auto_latched
                            && had_content
                            && now.duration_since(last_loud).as_secs_f32() >= silence_secs
                        {
                            auto_stopped = true;
                            log::info!(
                                "silence-stop: fired after {silence_secs}s silence (had content)"
                            );
                            force_idle();
                            auto_stop_pending_w.store(true, Ordering::SeqCst);
                            let _ = app_tx_hf
                                .send(AppMsg::Hotkey(HotkeyEvent::StopAndTranscribe));
                        }
                    }
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
    app_tx: &crossbeam_channel::Sender<AppMsg>,
    was_auto: bool,
    map_cache: &Arc<Mutex<postprocess::MapCache>>,
) {
    let cap = slot.lock().take();
    let Some(cap) = cap else {
        return;
    };
    overlay.hide();
    tray.set_active(false);
    let (whisper_cfg, language, continuous, newline_feed, command_mode, replace_maps_on, active_maps) = {
        let c = cfg.lock();
        (
            c.whisper.clone(),
            c.language.clone(),
            c.continuous,
            c.newline_feed,
            c.command_mode,
            c.replace_maps_enabled,
            c.enabled_replace_maps.clone(),
        )
    };
    let overlay_clone = overlay.clone();
    let app_tx_t = app_tx.clone();
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
            // In Loop mode, restart capture BEFORE the whisper round-trip so
            // the gap between utterances is just the device switchover, not
            // the full whisper + type latency.
            if was_auto && continuous {
                let _ = app_tx_t.send(AppMsg::RestartContinuous);
            }
            let loop_stitch = was_auto && continuous;
            let map = if replace_maps_on {
                map_cache.lock().get(&active_maps)
            } else {
                Arc::new(postprocess::ReplaceMap::default())
            };
            std::thread::spawn(move || {
                match whisper::transcribe(&wav, &language, &whisper_cfg) {
                    Ok(text) if !text.is_empty() => {
                        log::info!("transcript: {}", text);
                        let action_opt = postprocess::process_strict(&text, &map);
                        if action_opt.is_some() {
                            overlay_clone.replacement_hit();
                        }
                        let action = match (action_opt, command_mode) {
                            (Some(a), _) => a,
                            (None, true) => {
                                log::info!("command mode: no rule matched, dropped");
                                return;
                            }
                            (None, false) => Action::Text(text.clone()),
                        };
                        match action {
                            Action::Enter => inject::press_enter(),
                            Action::Text(raw) => {
                                let out = if loop_stitch {
                                    stitch_chunk(&raw)
                                } else {
                                    raw
                                };
                                inject::type_text(&out);
                                if newline_feed {
                                    inject::press_enter();
                                }
                            }
                            Action::Run(cmd) => run_shell(&cmd),
                            Action::Rewrite(url) => rewrite_selection(&url),
                            Action::Transform(name) => transform_selection(&name),
                            Action::Exec(cmd) => exec_selection(&cmd),
                            Action::Cmd(cmd) => exec_noinput(&cmd),
                            Action::Keys(seq) => inject::send_keys(&seq),
                        }
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

/// Write the captured WAV to `%APPDATA%\whisper-local\last.wav` (always —
/// single file, gets overwritten). When `WHISPER_DEBUG` is set, also write a
/// timestamped copy under `debug/<ts>.wav` so chunks can be replayed.
fn dump_wav_to_disk(wav: &[u8]) -> anyhow::Result<()> {
    let dir = whisper_local::config::config_dir()?;
    std::fs::create_dir_all(&dir)?;
    let last = dir.join("last.wav");
    std::fs::write(&last, wav)?;
    if std::env::var("WHISPER_DEBUG").is_ok() {
        let debug_dir = dir.join("debug");
        let _ = std::fs::create_dir_all(&debug_dir);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = std::fs::write(debug_dir.join(format!("{ts}.wav")), wav);
    }
    log::info!("wrote {}", last.display());
    Ok(())
}

/// Left-click on the tray icon: enter or leave "listen" mode. Toggles
/// continuous + command_mode together AND starts / stops capture so the
/// user doesn't have to press the chord for voice commands.
fn handle_toggle_listen(
    cfg: &Arc<Mutex<Config>>,
    slot: &Arc<Mutex<Option<AudioCapture>>>,
    overlay: &OverlayHandle,
    tray: &mut Tray,
    app_tx: &crossbeam_channel::Sender<AppMsg>,
    auto_stop_pending: &Arc<AtomicBool>,
    listen_saved_cmd: &Arc<Mutex<Option<bool>>>,
) {
    if !cfg.lock().left_click_listen {
        log::info!("listen: left-click disabled in settings");
        return;
    }
    let was_on = {
        let c = cfg.lock();
        c.continuous && c.command_mode
    };
    let turn_on = !was_on;
    {
        let mut c = cfg.lock();
        if turn_on {
            *listen_saved_cmd.lock() = Some(c.command_mode);
            c.continuous = true;
            c.command_mode = true;
        } else {
            c.continuous = false;
            c.command_mode = listen_saved_cmd.lock().take().unwrap_or(false);
        }
        let _ = c.save();
    }
    tray.set_command_mode(cfg.lock().command_mode);
    tray.set_command_mode_locked(turn_on);
    log::info!("listen: {}", if turn_on { "on" } else { "off" });
    if turn_on {
        start_capture(cfg, slot, overlay, true, tray, app_tx, auto_stop_pending);
        force_latch();
    } else {
        discard(slot, overlay, tray);
        force_idle();
    }
}

/// Wrap this process in a Job Object with KILL_ON_JOB_CLOSE so all child
/// processes (settings, transcribe-file, overlay) die when the tray exits or
/// crashes. The job handle is intentionally leaked: it stays alive for the
/// lifetime of this process and the OS closes it (firing the kill) on exit.
fn bind_children_to_job() {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::*;
    use windows::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let job = match CreateJobObjectW(None, windows::core::PCWSTR::null()) {
            Ok(h) if !h.is_invalid() => h,
            Ok(_) | Err(_) => {
                log::warn!("CreateJobObjectW failed; child cleanup disabled");
                return;
            }
        };
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .is_err()
        {
            log::warn!("SetInformationJobObject failed");
            return;
        }
        let me: HANDLE = GetCurrentProcess();
        if AssignProcessToJobObject(job, me).is_err() {
            log::warn!("AssignProcessToJobObject failed");
            return;
        }
        // HANDLE is Copy with no Drop — the OS keeps the handle open until
        // CloseHandle or process exit. Process exit closes every handle, which
        // triggers KILL_ON_JOB_CLOSE on the job, which kills all members
        // (us + every child we spawned). Exactly what we want.
        let _keep_alive = job;
        log::info!("bound to job object — children die when tray exits");
    }
}

/// On first launch (or on a fresh install), make sure `replace_maps/` exists
/// and is seeded with `global.txt`, `medical.txt`, `legal.txt`. Also migrate
/// the legacy root `replace_map.txt` into `replace_maps/global.txt` if the
/// new file isn't there yet.
fn bootstrap_replace_maps() {
    let Some(dir) = postprocess::replace_maps_dir() else {
        return;
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::error!("create replace_maps dir: {e}");
        return;
    }
    let global = dir.join("global.txt");
    if !global.exists() {
        // Migrate legacy file if present.
        let legacy = whisper_local::config::config_dir()
            .ok()
            .map(|d| d.join("replace_map.txt"));
        if let Some(legacy) = legacy.filter(|p| p.exists()) {
            let _ = std::fs::copy(&legacy, &global);
            log::info!("migrated {} -> {}", legacy.display(), global.display());
        } else {
            let _ = std::fs::write(
                &global,
                include_str!("../templates/replace_maps/global.txt"),
            );
        }
    }
    for (name, contents) in [
        ("medical.txt", include_str!("../templates/replace_maps/medical.txt")),
        ("legal.txt", include_str!("../templates/replace_maps/legal.txt")),
        ("programming.txt", include_str!("../templates/replace_maps/programming.txt")),
        ("launch.txt", include_str!("../templates/replace_maps/launch.txt")),
        ("ai.txt", include_str!("../templates/replace_maps/ai.txt")),
    ] {
        let p = dir.join(name);
        if !p.exists() {
            let _ = std::fs::write(&p, contents);
        }
    }
    if let Ok(cfg_dir) = whisper_local::config::config_dir() {
        let helpers = cfg_dir.join("helpers");
        let _ = std::fs::create_dir_all(&helpers);
        for (name, contents) in [
            ("_common.py",     include_str!("../templates/helpers/_common.py")),
            ("claude.ps1",     include_str!("../templates/helpers/claude.ps1")),
            ("claude.py",      include_str!("../templates/helpers/claude.py")),
            ("openai.ps1",     include_str!("../templates/helpers/openai.ps1")),
            ("openai.py",      include_str!("../templates/helpers/openai.py")),
            ("openrouter.ps1", include_str!("../templates/helpers/openrouter.ps1")),
            ("openrouter.py",  include_str!("../templates/helpers/openrouter.py")),
            ("ollama.ps1",     include_str!("../templates/helpers/ollama.ps1")),
            ("ollama.py",      include_str!("../templates/helpers/ollama.py")),
            ("lmstudio.ps1",   include_str!("../templates/helpers/lmstudio.ps1")),
            ("lmstudio.py",    include_str!("../templates/helpers/lmstudio.py")),
            ("vllm.ps1",       include_str!("../templates/helpers/vllm.ps1")),
            ("vllm.py",        include_str!("../templates/helpers/vllm.py")),
            ("llamacpp.ps1",   include_str!("../templates/helpers/llamacpp.ps1")),
            ("llamacpp.py",    include_str!("../templates/helpers/llamacpp.py")),
        ] {
            let p = helpers.join(name);
            if !p.exists() {
                let _ = std::fs::write(&p, contents);
            }
        }
    }
}

/// Read the current selection (Ctrl+C → clipboard), apply a built-in transform,
/// type the result back over the selection.
fn transform_selection(name: &str) {
    let name = name.to_string();
    log::info!("transform_selection: {name}");
    std::thread::spawn(move || {
        inject::send_copy();
        std::thread::sleep(std::time::Duration::from_millis(120));
        let s = match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(s) => s,
            Err(e) => {
                log::error!("transform_selection: clipboard read: {e}");
                return;
            }
        };
        let out = apply_transform(&name, &s);
        match out {
            Some(out) => inject::type_text(&out),
            None => log::warn!("transform_selection: unknown transform `{name}`"),
        }
    });
}

fn apply_transform(name: &str, s: &str) -> Option<String> {
    use sha2::Digest;
    Some(match name {
        "lower" | "lowercase" => s.to_lowercase(),
        "upper" | "uppercase" => s.to_uppercase(),
        "trim" => s.trim().to_string(),
        "reverse" => s.chars().rev().collect(),
        "md5" => format!("{:x}", md5::Md5::digest(s.as_bytes())),
        "sha256" => format!("{:x}", sha2::Sha256::digest(s.as_bytes())),
        _ => return None,
    })
}

/// Spawn a shell command via `cmd /c`. Used by replace_map `!`-prefixed entries
/// to launch programs by voice (e.g. `start battlefield:!"C:\bf.exe"`).
/// URLs (or `start "" "<url>"` patterns) skip cmd.exe and go through
/// `webbrowser::open`, which dodges cmd's ugly quoting rules. Other commands
/// reach cmd via `raw_arg` so the verbatim line is preserved.
fn run_shell(cmd: &str) {
    use std::os::windows::process::CommandExt;
    let cmd = cmd.to_string();
    log::info!("run_shell: {cmd}");
    if let Some(url) = extract_url_command(&cmd) {
        std::thread::spawn(move || {
            if let Err(e) = webbrowser::open(&url) {
                log::error!("run_shell open url `{url}`: {e}");
            }
        });
        return;
    }
    std::thread::spawn(move || {
        let r = std::process::Command::new("cmd")
            .raw_arg(format!("/c {cmd}"))
            .spawn();
        if let Err(e) = r {
            log::error!("run_shell spawn: {e}");
        }
    });
}

/// Recognise common URL-launch patterns so we can call `webbrowser::open`
/// directly instead of routing through cmd.exe (whose quote handling makes
/// `start "" "https://..."` open `\\` half the time).
fn extract_url_command(cmd: &str) -> Option<String> {
    let t = cmd.trim();
    if let Some(stripped) = strip_http_url(t) {
        return Some(stripped);
    }
    let mut rest = t.strip_prefix("start ").or_else(|| t.strip_prefix("START "))?;
    rest = rest.trim_start();
    if let Some(after_empty_title) = rest.strip_prefix("\"\"") {
        rest = after_empty_title.trim_start();
    }
    let unquoted = rest
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .unwrap_or(rest);
    strip_http_url(unquoted)
}

fn strip_http_url(s: &str) -> Option<String> {
    let s = s.trim();
    if s.starts_with("http://") || s.starts_with("https://") {
        Some(s.to_string())
    } else {
        None
    }
}

/// Run a command with no stdin, capture stdout, type the result at the
/// caret. Used by replace_map `>>cmd:` entries — where the input was baked
/// into the command line via regex captures, not taken from a selection.
fn exec_noinput(cmd: &str) {
    use std::os::windows::process::CommandExt;
    let cmd = cmd.to_string();
    log::info!("exec_noinput: {cmd}");
    std::thread::spawn(move || {
        let out = std::process::Command::new("cmd")
            .raw_arg(format!("/c {cmd}"))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output();
        let out = match out {
            Ok(o) => o,
            Err(e) => {
                log::error!("exec_noinput spawn: {e}");
                return;
            }
        };
        if !out.status.success() {
            log::error!(
                "exec_noinput non-zero exit: {}, stderr={:?}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            );
            return;
        }
        let body = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
        if body.is_empty() {
            log::warn!("exec_noinput: empty stdout");
            return;
        }
        inject::type_text(&body);
    });
}

/// Read the current selection (Ctrl+C → clipboard), pipe it into a local
/// command as stdin, type the command's stdout back over the selection.
/// Used by replace_map `>>exec:` entries.
fn exec_selection(cmd: &str) {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    let cmd = cmd.to_string();
    log::info!("exec_selection: {cmd}");
    std::thread::spawn(move || {
        inject::send_copy();
        std::thread::sleep(std::time::Duration::from_millis(120));
        let selection = match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(s) => s,
            Err(e) => {
                log::error!("exec_selection: clipboard read: {e}");
                return;
            }
        };
        let mut child = match std::process::Command::new("cmd")
            .raw_arg(format!("/c {cmd}"))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                log::error!("exec_selection spawn: {e}");
                return;
            }
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(selection.as_bytes());
        }
        let out = match child.wait_with_output() {
            Ok(o) => o,
            Err(e) => {
                log::error!("exec_selection wait: {e}");
                return;
            }
        };
        if !out.status.success() {
            log::error!(
                "exec_selection non-zero exit: {}, stderr={:?}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            );
            return;
        }
        let body = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
        if body.is_empty() {
            log::warn!("exec_selection: empty stdout");
            return;
        }
        inject::type_text(&body);
    });
}

/// Send Ctrl+C to copy the current selection, POST it to `url` as a plain-text
/// body, then type the response back. Used by replace_map `>>`-prefixed entries.
fn rewrite_selection(url: &str) {
    let url = url.to_string();
    log::info!("rewrite_selection: {url}");
    std::thread::spawn(move || {
        inject::send_copy();
        std::thread::sleep(std::time::Duration::from_millis(120));
        let selection = match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(s) => s,
            Err(e) => {
                log::error!("rewrite_selection: clipboard read failed: {e}");
                return;
            }
        };
        if selection.trim().is_empty() {
            log::warn!("rewrite_selection: empty selection, skipping POST");
            return;
        }
        let resp = match reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .and_then(|c| {
                c.post(&url)
                    .header("Content-Type", "text/plain; charset=utf-8")
                    .body(selection)
                    .send()
            }) {
            Ok(r) => r,
            Err(e) => {
                log::error!("rewrite_selection: POST {url}: {e}");
                return;
            }
        };
        if !resp.status().is_success() {
            log::error!("rewrite_selection: HTTP {}", resp.status());
            return;
        }
        match resp.text() {
            Ok(body) => inject::type_text(&body),
            Err(e) => log::error!("rewrite_selection: read body: {e}"),
        }
    });
}

/// Loop-mode chunks come back as mini-sentences ("Hello world.") which causes
/// periods to pile up and spaces between chunks to vanish. Strip the trailing
/// sentence terminator Whisper tacks on, collapse surrounding whitespace, and
/// append a single space so the next chunk lands cleanly after this one.
fn stitch_chunk(text: &str) -> String {
    let trimmed = text.trim();
    let stripped = trimmed.trim_end_matches(|c: char| matches!(c, '.' | '!' | '?'));
    let base = stripped.trim_end();
    format!("{base} ")
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
