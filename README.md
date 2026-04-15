# whisper-local

**Local-first voice-to-text for Windows 11.** Tray app that records on a global hotkey and types the transcript into the focused window. Audio never leaves your machine — it's POSTed to a Whisper server you control.

```
Ctrl+Win  hold to record   →  release to transcribe + type
Ctrl+Win  double-tap        →  latched mode; tap again to stop
double-click tray icon      →  open drag-and-drop file transcriber
```

## Why local-first

Cloud transcription means uploading every recording to a third party. whisper-local talks to a Whisper server **on your own machine** (Docker container or WSL2). Your voice and any dropped audio/video stay on your hardware. No API keys, no rate limits, no transcripts in someone else's logs.

## Requirements

- **Windows 11**
- A locally running **Whisper HTTP server** that exposes `POST /v1/audio/transcriptions` (OpenAI-compatible) and `GET /health`. Default target: `http://localhost:10010`. Tested with **faster-whisper-large-v3-turbo** in Docker / WSL2.
- (Optional) any process manager at `http://localhost:9999/start` to auto-wake Whisper on demand.

## Features

- Hotkey-driven mic capture (push-to-talk + latched).
- Drag-and-drop file transcriber: audio and video, any format ffmpeg understands.
- Language picker (auto-detect or pick ISO code).
- Optional speaker diarization (Auto / Exactly N / Pitch-based 2-speaker).
- Per-speaker copy + save (`.txt`) when multiple speakers detected.
- Live floating overlay with mic-level waveform.
- CJK + Cyrillic + Hangul rendering (Segoe UI + system CJK fonts loaded as fallback).

## Build

```bash
cargo build --release                                                    # full (~10 MB)
cargo build --release --no-default-features --features transcribe-file   # min:  no speaker UI
cargo build --release --no-default-features                              # pure: mic + tray only
```

## Run

```
./target/release/whisper-local.exe
```

Right-click the tray icon → **Settings** to pick microphone, set the Whisper URL, choose a language, toggle autostart, or enable speaker detection.

## Config

`%APPDATA%\whisper-local\config.toml` — auto-created with defaults.

## Log

`%APPDATA%\whisper-local\log.txt` — rotated at 1 MB. `WHISPER_DEBUG=1` for verbose.

## License

MIT.
