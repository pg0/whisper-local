<h1 align="center">whisper-local</h1>

<p align="center"><strong>Hold a key. Speak. Release. It types.</strong></p>

<p align="center">
  Lightweight • Private • Local. A Windows tray app that turns your microphone into a keyboard
  in any application, talking to a Whisper server you run yourself in Docker or WSL2.
  Audio never leaves your machine.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Windows-11-0078D4?style=for-the-badge&logo=windows&logoColor=white" alt="Windows 11">
  <img src="https://img.shields.io/badge/Rust-1.75+-CE422B?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Whisper-large--v3--turbo-7C3AED?style=for-the-badge" alt="Whisper">
  <img src="https://img.shields.io/badge/Backend-Docker_or_WSL2-2496ED?style=for-the-badge&logo=docker&logoColor=white" alt="Docker / WSL2">
  <img src="https://img.shields.io/badge/License-MIT-yellow?style=for-the-badge" alt="MIT">
</p>

<p align="center">
  <img src="https://img.shields.io/badge/binary-3.6_to_10_MB-success?style=flat" alt="Binary">
  <img src="https://img.shields.io/badge/idle_RAM-15_MB-success?style=flat" alt="RAM">
  <img src="https://img.shields.io/badge/no_telemetry-✓-blue?style=flat" alt="No telemetry">
  <img src="https://img.shields.io/badge/no_account-✓-blue?style=flat" alt="No account">
  <img src="https://img.shields.io/badge/no_subscription-✓-blue?style=flat" alt="No subscription">
</p>

```
Ctrl+Win  hold to record    →  release, transcript types into the focused app
Ctrl+Win  double-tap        →  latched mode; tap again to stop
double-click tray icon      →  drag-and-drop file transcriber
right-click tray icon       →  microphone, language, settings
```

> **Auto-stop** *(optional, Settings)* — Hold `Ctrl+Win` and keep holding. After
> **2 seconds** (editable) it auto-latches so you can release. After **5 seconds of
> silence** (editable) recording stops and the transcript is typed out. Silence is
> defined by an editable RMS threshold (default `0.01`). One-shot: typing ends the
> session. Off by default.
>
> **Loop (continuous hands-free)** *(optional, Settings)* — When Auto-stop is also on,
> the app restarts recording automatically right after each transcript is typed, in
> latched state. Keep talking, pause, keep talking, pause… each chunk gets typed.
> Press `Ctrl+Win` once to break out. Needs Auto-stop to detect when each utterance
> ends. Off by default.

---

## ✨ Key features

<table align="center">
<tr>
<td align="center" width="20%" valign="top">
<h3>🎙️ Push-to-talk</h3>
Hold <code>Ctrl+Win</code>, speak, release.<br>
Transcript types into the focused window via <code>SendInput</code>.<br>
Latched mode for hands-free.
</td>
<td align="center" width="20%" valign="top">
<h3>🔒 Local-only</h3>
Audio never leaves your machine.<br>
Talks to <strong>your</strong> Whisper server on <code>localhost</code>.<br>
No accounts, no telemetry, no rate limits.
</td>
<td align="center" width="20%" valign="top">
<h3>📂 File drop</h3>
Drag any audio or video file onto a small window.<br>
Save as <code>.txt</code> or copy to clipboard.<br>
ffmpeg-supported formats.
</td>
<td align="center" width="20%" valign="top">
<h3>🌍 Multilingual</h3>
Auto-detect or pin an ISO code.<br>
English, Deutsch, 中文, 日本語, 한국어, … render correctly with system fonts.
</td>
<td align="center" width="20%" valign="top">
<h3>👥 Speakers</h3>
Optional diarization.<br>
Auto / Exactly N / Pitch-based.<br>
Per-speaker copy + save once detected.
</td>
</tr>
</table>

---

## 🤔 Why local-first

Most voice-to-text products ship your microphone to a server farm and your transcripts to a database
you can't see. Some go further and screenshot your active window for "context." That's an unusual
permission model for something you talk to all day.

`whisper-local` goes the other way:

- **Your audio never leaves your machine.** It's a `localhost` POST to a Whisper server you started.
- **No telemetry. No accounts. No rate limits. No subscription.**
- **No screen capture. No clipboard sniffing. No background context-gathering.** It records when you hold the hotkey and shuts up when you release it.
- **Tiny footprint.** A single ~10 MB tray binary. Mainstream cloud dictation tools ship as 500+ MB Electron apps that idle in the background; this is one process with one job.
- **You own the model.** Swap `faster-whisper-large-v3-turbo` for any OpenAI-compatible Whisper server you trust.

If your machine is offline, it still works — as long as your Whisper server is reachable on `localhost`.

---

## 🚀 Quick start

1. Run a local Whisper server on `http://localhost:10010` (Docker / WSL2). Tested with **faster-whisper-large-v3-turbo**.
2. Download the latest `whisper-local.exe` from [Releases](../../releases) (or build from source — see below).
3. Run it. Tray icon appears.
4. Right-click the tray → **Settings** → set Whisper URL, microphone, language.
5. Hold **Ctrl+Win** anywhere → speak → release. Transcript types into whatever window has focus.
6. Double-click the tray → drag any audio / video file onto the small window for offline file transcription.

---

## 📦 Requirements

- **Windows 11**
- A locally running **Whisper HTTP server** exposing OpenAI-compatible endpoints:
  - `POST /v1/audio/transcriptions` (multipart audio file in)
  - `GET  /health`
- Default target: `http://localhost:10010`. Configurable in Settings.
- Tested against **faster-whisper-large-v3-turbo** running in Docker on the same machine, and the same model running in WSL2 with CUDA.

That's it. No login. No cloud account. No paid tier.

---

## 🔨 Build

```bash
cargo build --release                                                    # full
cargo build --release --no-default-features --features transcribe-file   # min
```

| build | binary | idle RAM | overlay | drop window | speakers | Settings UI |
|-------|--------|----------|---------|-------------|----------|-------------|
| **full** | 10 MB | ~380 MB | ✓ | ✓ | ✓ | ✓ |
| **min**  | 10 MB | ~15 MB  | — | ✓ | — | ✓ |

Min skips the always-on overlay child process — that drops idle RAM from ~380 MB to ~15 MB. Open the file-drop window only when you need it (a transient ~250 MB while it's on screen, then back to 15 MB).

---

## 🗂️ Config

`%APPDATA%\whisper-local\config.toml` — auto-created with sane defaults. Edit it directly or use the Settings window.

## 📓 Log

`%APPDATA%\whisper-local\log.txt` — rotated at 1 MB. `WHISPER_DEBUG=1` for verbose output.

---

## ⚖️ License

MIT.
