use std::time::{Duration, Instant};
use whisper_local::hotkey::{HotkeyEvent, KeyEvent, Machine, VKey};

fn t0() -> Instant { Instant::now() }

#[test]
fn hold_to_talk_records_and_transcribes() {
    let mut m = Machine::default();
    let start = t0();
    assert_eq!(m.on(KeyEvent::Down(VKey::LCtrl, start)), None);
    let ev = m.on(KeyEvent::Down(VKey::LWin, start));
    assert_eq!(ev, Some(HotkeyEvent::StartRecording));
    let later = start + Duration::from_millis(800);
    let ev = m.on(KeyEvent::Up(VKey::LWin, later));
    assert_eq!(ev, Some(HotkeyEvent::StopAndTranscribe));
}

#[test]
fn double_tap_latch_flow() {
    let mut m = Machine::default();
    let t = t0();
    m.on(KeyEvent::Down(VKey::LCtrl, t));
    assert_eq!(m.on(KeyEvent::Down(VKey::LWin, t)), Some(HotkeyEvent::StartRecording));
    let t2 = t + Duration::from_millis(100);
    // Tap end: release Win, Ctrl still down -> chord inactive, state MaybeDoubleTap
    assert_eq!(m.on(KeyEvent::Up(VKey::LWin, t2)), Some(HotkeyEvent::DiscardRecording));
    // Second tap: press Win again with Ctrl still down
    let t3 = t2 + Duration::from_millis(150);
    assert_eq!(m.on(KeyEvent::Down(VKey::LWin, t3)), Some(HotkeyEvent::StartLatched));
    // Stop: release all, press chord again
    let t4 = t3 + Duration::from_millis(5000);
    m.on(KeyEvent::Up(VKey::LWin, t4));
    m.on(KeyEvent::Up(VKey::LCtrl, t4));
    let t5 = t4 + Duration::from_millis(100);
    m.on(KeyEvent::Down(VKey::LCtrl, t5));
    assert_eq!(m.on(KeyEvent::Down(VKey::LWin, t5)), Some(HotkeyEvent::StopAndTranscribe));
}

#[test]
fn third_key_discards_recording() {
    let mut m = Machine::default();
    let t = t0();
    m.on(KeyEvent::Down(VKey::LCtrl, t));
    m.on(KeyEvent::Down(VKey::LWin, t));
    let ev = m.on(KeyEvent::Down(VKey::Other, t + Duration::from_millis(50)));
    assert_eq!(ev, Some(HotkeyEvent::DiscardRecording));
}

#[test]
fn double_tap_window_expires_to_idle() {
    let mut m = Machine::default();
    let t = t0();
    m.on(KeyEvent::Down(VKey::LCtrl, t));
    m.on(KeyEvent::Down(VKey::LWin, t));
    m.on(KeyEvent::Up(VKey::LWin, t + Duration::from_millis(50)));
    // expire
    let ev = m.double_tap_expired(t + Duration::from_millis(500));
    assert_eq!(ev, Some(HotkeyEvent::MaybeDoubleTapExpired));
}
