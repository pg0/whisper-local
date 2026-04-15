# whisper-local

**Hold a key. Speak. Release. It types.**

Lightweight • Private • Local

A Windows tray app that turns your microphone into a keyboard, anywhere on your system. Audio never leaves your machine — every transcription is POSTed to a Whisper server *you* run, in Docker or WSL2 on the same box.

```
Ctrl+Win  hold to record    →  release, transcript types into the focused app
Ctrl+Win  double-tap        →  latched mode; tap again to stop
double-click tray icon      →  drag-and-drop file transcriber
```

## Why local-first

Most voice-to-text products ship your microphone to a server farm and your transcripts to a database you can't see. Some go further and screenshot your active window for "context." That's an unusual permission model for something you talk to all day.

whisper-local goes the other way:

- **Your audio never leaves your machine.** It's a localhost POST to a Whisper server you started.
- **No telemetry. No accounts. No rate limits. No subscription.**
- **No screen capture. No clipboard sniffing. No background context-gathering.** It records when you hold the hotkey and shuts up when you release it.
- **Tiny footprint.** A single ~10 MB tray binary. Mainstream cloud dictation tools ship as 500+ MB Electron apps that idle in the background; whisper-local is one process with one job.
- **You own the model.** Swap `faster-whisper-large-v3-turbo` for any OpenAI-compatible Whisper server you trust.

If your machine is offline, it still works — as long as your Whisper server is reachable on `localhost`.

## What you get

- **Push-to-talk dictation** — hold `Ctrl+Win`, speak, release. Transcript types into whatever window has focus: editor, browser, terminal, Slack, anywhere `SendInput` reaches.
- **Latched mode** — double-tap the hotkey for hands-free recording; tap again to stop.
- **Drag-and-drop file transcriber** — drop any audio or video file (mp3, wav, m4a, mp4, mkv, webm, ogg, flac, opus, …) onto a small window and get a `.txt` or clipboard copy.
- **Language picker** — auto-detect or pin an ISO code (English, Deutsch, Français, Español, Italiano, Português, Polski, Русский, 中文, 日本語, 한국어, …). Native scripts render correctly.
- **Optional speaker diarization** — when enabled in settings: Auto, Exactly N, or Pitch-based 2-speaker. Per-speaker copy + save once detected.
- **Microphone picker, autostart, custom Whisper URL** — all in a small settings window. No web dashboard.
- **Three build flavours** — full, minimal (no speaker UI), pure (just the hotkey + mic).

## Requirements

- **Windows 11**
- A locally running **Whisper HTTP server** exposing OpenAI-compatible endpoints:
  - `POST /v1/audio/transcriptions` (multipart audio file in)
  - `GET  /health`
- Default target: `http://localhost:10010`. Configurable in Settings.
- Tested against **faster-whisper-large-v3-turbo** running in Docker on the same machine, and the same model running in WSL2 with CUDA.

That's it. No login. No cloud account. No paid tier.

## Build

```bash
cargo build --release                                                    # full
cargo build --release --no-default-features --features transcribe-file   # min:  no speaker UI
cargo build --release --no-default-features                              # pure: mic + tray only
```

Three binaries land in `target/release/`. Pick the one that matches what you need; the rest stays out of your taskbar and out of your RAM.

## Run

```
./target/release/whisper-local.exe
```

A tray icon appears. Right-click → **Settings** to pick a microphone, set the Whisper URL, choose a language, toggle autostart, or enable speaker detection.

Then forget it's there until you press `Ctrl+Win`.

## Config

`%APPDATA%\whisper-local\config.toml` — auto-created with sane defaults. Edit it directly or use the Settings window.

## Log

`%APPDATA%\whisper-local\log.txt` — rotated at 1 MB. `WHISPER_DEBUG=1` for verbose output.

## Status

Early. The core flow (hotkey → record → Whisper → type) is solid; the file-drop window is solid. The floating recording overlay is currently disabled while a multi-window threading bug gets sorted — for now the tray icon swaps colour when recording. Expect the rough edges of a one-person tool that's still shaping up.

## License

MIT.
