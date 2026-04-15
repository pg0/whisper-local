# CHANGELOG

## 20260414 (Task 11)
- **Files changed:** `src/main.rs`
- **What changed:** Task 11 — Replaced stub main.rs with full controller loop. Wires hotkey, audio, whisper, overlay, tray, and settings. `--settings` child-process branch spawns settings UI and exits. Main loop polls tray via `Tray::try_recv()` every 100ms timeout cycle. `AudioCapture::stop()` called on main thread (cpal::Stream is !Send); WAV bytes handed off to background thread for Whisper HTTP call. Settings spawned as child process; on exit sends `ReloadConfig`. Log rotation at 1 MiB.

## 20260414
- **Files changed:** `src/tray.rs`, `src/settings_ui.rs`, `src/lib.rs`, `assets/tray_idle.png`, `assets/tray_active.png`, `scripts/make_icons.py`
- **What changed:** Task 10 — Added `tray` module with system-tray icon (idle grey / active red 32x32 PNGs), right-click menu (Settings, Quit), `TrayEvent` enum, `try_recv()` poll method, and `set_active()`. Added `settings_ui` module with egui window: mic ComboBox, Whisper URL field, autostart checkbox, health indicator (polling every 2 s), Save/Cancel buttons. API fixes vs template: `from_id_salt`→`from_id_source` (egui 0.27), `MenuItem::new` third arg is `Option<Accelerator>` (not `Option<&str>`), used `MenuEvent::receiver()` global channel instead of `set_event_handler`.

## 20260414
- **Files changed:** `src/autostart.rs`, `src/lib.rs`
- **What changed:** Task 8 — Added `autostart` module with `is_enabled()`, `set_enabled()`, and `current_exe_path()` using Win32 HKCU Run registry key. Added `pub mod autostart;` to lib.rs.

## 20260414
- **Files changed:** `src/overlay.rs`, `src/lib.rs`
- **What changed:** Task 9 — Added `overlay` module: borderless always-on-top click-through egui pill with waveform bars and error state. `OverlayHandle` exposes show_recording/show_latched/show_error/push_rms/hide/quit. Added `pub mod overlay;` to lib.rs.

## 20260414
- **Files changed:** `src/inject.rs`, `src/lib.rs`
- **What changed:** Task 5 — Added `inject` module with `type_text()` using Win32 SendInput Unicode key events, chunked in batches of 40 with 1ms sleep between batches. Added `pub mod inject;` to lib.rs.

## 20260414
- **Files changed:** `src/audio.rs`, `src/lib.rs`, `tests/audio_wav_test.rs`
- **What changed:** Task 6 — Added `audio` module with cpal input capture (F32/I16 sample formats), crossbeam-channel RMS metering, parking_lot buffer, rubato SincFixedIn resampler to 16kHz mono, and hound WAV encoder. Added `pub mod audio;` to lib.rs. Unit test verifies RIFF/WAVE header and minimum output length.

- **Files changed:** `docs/superpowers/specs/2026-04-14-wispr-local-design.md`
- **What changed:** Initial design spec for wispr-local — a minimal Windows tray app that records voice on Ctrl+Win, transcribes via local whisper HTTP server (localhost:10010), and types the transcript via SendInput. Why: user wants a small/simple local Wispr Flow clone with hold-to-talk + double-tap-latch semantics.

- **Files changed:** `src/hotkey/hook.rs`, `src/hotkey/mod.rs`, `src/main.rs`
- **What changed:** Task 4 — WH_KEYBOARD_LL hook wired to state machine. Created `hook.rs` with low-level keyboard hook, SHARED OnceCell, timer thread for double-tap expiry, and message pump thread. Updated `mod.rs` to export `hook` module and `spawn_hook`. Updated `main.rs` with smoke-test harness logging "hook installed".
- **Files changed:** `docs/superpowers/specs/2026-04-14-wispr-local-design.md`
- **What changed:** Added second Settings UI field — Whisper server URL — and refactored config to store `whisper.base_url` instead of hard-coded `transcribe_url`/`health_url` (both derived from base). Why: user requested a settings option to point at a different whisper server without editing config.toml.
- **Files changed:** `docs/superpowers/plans/2026-04-14-wispr-local-plan.md`
- **What changed:** Wrote 12-task implementation plan covering cargo scaffold, config, hotkey state machine + hook, audio capture + WAV, whisper client, autostart, overlay pill, tray + settings UI, main controller loop, README + changelog. Why: turn design spec into step-by-step executable plan.

## 20260414 (final)
- **Files changed:** `README.md`
- **What changed:** v0.1 release — added user-facing README covering hotkey behavior, build, run, config, and log paths. Why: document basic usage so a user can run the binary without the spec/plan.
