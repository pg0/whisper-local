# CHANGELOG

## 20260415 (v0.1.1)
- **Files changed:** `Cargo.toml`, `Cargo.lock`
- **What changed:** Version bump 0.1.0 → 0.1.1 for the hands-free release.

## 20260415 (Hands-free)
- **Files changed:** `src/config.rs`, `src/settings_ui.rs`, `src/hotkey/state.rs`, `src/hotkey/hook.rs`, `src/hotkey/mod.rs`, `src/main.rs`, `README.md`
- **What changed:** Added optional hands-free mode. Settings gains checkbox "Hands-free (auto-latch on hold, auto-stop on silence)" with three editable numeric fields: auto-latch hold seconds (default 2.0), auto-stop silence seconds (default 5.0), silence RMS threshold (default 0.01). RMS-consumer thread in `start_capture` now also watches wall-clock for auto-latch (calls new `hotkey::force_latch`, shows latched overlay) and last-loud time for auto-stop (calls `force_idle` + sends `StopAndTranscribe` via app channel). Each numeric field renders a "(default: …)" weak label beside it; Whisper URL field got the same hint. Settings window widened to 560×460. README gained a hands-free block after the shortcut table. Config defaults are conservative so behaviour stays opt-in.

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

## 20260415
- **Files changed:** `Cargo.toml`, `src/lib.rs`, `src/main.rs`, `src/overlay.rs`, `src/settings_ui.rs`, `src/transcribe_ui.rs`, `src/tray.rs`, `src/whisper.rs`, `src/config.rs`, `src/fonts.rs`, `tests/whisper_test.rs`, `README.md`, `.gitignore`
- **What changed:** Major polish + structural changes for v0.1.0 public release:
  - **Renamed** project `wispr-local` → `whisper-local` everywhere (Cargo package + lib + bin name, AppData dir, autostart registry value `WhisperLocal`, env var `WHISPER_DEBUG`, tray tooltip, window titles, single-instance mutex). Reason: avoid the WISPR FLOW trademark zone (USPTO 99560508).
  - **Cargo features** — `gui` (eframe + egui + rfd, optional deps), `overlay-ui`, `transcribe-file`, `speaker-detection`. Default = all. `--no-default-features --features transcribe-file` produces the **min** build (no overlay child); `--no-default-features` would produce a tray-only "lite" but is not currently shipped.
  - **Speaker-detection** — new `SpeakerMode` enum (Off / AutoMin / Exact / GenderOnly mapped to whisper API form fields `diarize`, `min_speakers`, `num_speakers`, `pitch=true`). Per-speaker copy + save UI in transcribe-file window.
  - **Language picker** — added shared `config::LANGUAGES` const (13 entries native-script + ISO code), Settings dropdown, transcribe-file window combo, **and** a tray right-click "Language" submenu (radio-group). `whisper::transcribe` and `whisper::transcribe_file_verbose` now both accept a language param sent as the `language` form field.
  - **CJK / Cyrillic / Hangul** rendering via shared `fonts::install_broad_unicode_font` (Segoe UI + Microsoft YaHei + Malgun, prepended to egui's font fallback chain) used by both transcribe-file and overlay windows.
  - **Overlay** — moved from worker-thread spawn (panicked on Windows) to a child process (`whisper-local.exe --overlay`) with stdin text-line protocol (REC / LAT / RMS\t<f> / ERR\t<msg> / HID / QUI). Shrunk from 420×68 to 280×44; recording dot is painter-drawn (no font glyph dependency); uses wgpu (glow tested but does not paint visible windows on this Windows config — see `~/.claude/lessons_learned_coding.md`).
  - **Tray** — Microphone submenu reordered below Language. New `TrayEvent::SelectLanguage(String)` wired to live config save.
  - **Repo housekeeping** — full git-history nuke + single-commit re-init twice (once for trademark cleanup, once for clean release). `docs/superpowers/` removed and added to .gitignore. Repo at `pg0/whisper-local`, public.
  - **README** — full rewrite in OpenHarness-style polished form: centered title + tagline, badge rows, 5-column feature table, "Why local-first" pitch, Quick start, Build matrix, Requirements, Config, Log, License.
  - **v0.1.0 GitHub release** tagged + published with `whisper-local.exe` (full) and `whisper-local.min.exe` attached.
- **Why:** turn the working prototype into something publishable: legally-safe name, polished landing page, two clearly-documented build flavours, honest list of known gaps (no live word-by-word streaming, no vocabulary presets, overlay 360 MB on wgpu).
