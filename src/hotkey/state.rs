use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VKey {
    LCtrl,
    RCtrl,
    LWin,
    RWin,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    Down(VKey, Instant),
    Up(VKey, Instant),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    StartRecording,
    StartLatched,
    StopAndTranscribe,
    DiscardRecording,
    MaybeDoubleTapExpired, // timer notification
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum S {
    Idle,
    Recording { since: Instant },
    MaybeDoubleTap { since: Instant },
    Latched,
}

pub struct Machine {
    ctrl: bool,
    win: bool,
    chord_active: bool, // both ctrl and win down
    state: S,
    pub short_press_ms: u64,  // < this = tap (default 250)
    pub double_tap_window_ms: u64, // window for second tap (default 400)
}

impl Default for Machine {
    fn default() -> Self {
        Self {
            ctrl: false,
            win: false,
            chord_active: false,
            state: S::Idle,
            short_press_ms: 250,
            double_tap_window_ms: 400,
        }
    }
}

impl Machine {
    pub fn on(&mut self, ev: KeyEvent) -> Option<HotkeyEvent> {
        let (v, is_down, now) = match ev {
            KeyEvent::Down(v, t) => (v, true, t),
            KeyEvent::Up(v, t) => (v, false, t),
        };
        let was_chord = self.chord_active;
        match v {
            VKey::LCtrl | VKey::RCtrl => self.ctrl = is_down,
            VKey::LWin | VKey::RWin => self.win = is_down,
            VKey::Other => {
                // Any non-modifier keydown while recording -> discard
                if is_down {
                    match self.state {
                        S::Recording { .. } | S::Latched => {
                            self.state = S::Idle;
                            return Some(HotkeyEvent::DiscardRecording);
                        }
                        _ => {}
                    }
                }
                return None;
            }
        }
        self.chord_active = self.ctrl && self.win;

        if !was_chord && self.chord_active {
            return self.on_chord_press(now);
        }
        if was_chord && !self.chord_active {
            return self.on_chord_release(now);
        }
        None
    }

    fn on_chord_press(&mut self, now: Instant) -> Option<HotkeyEvent> {
        match self.state {
            S::Idle => {
                self.state = S::Recording { since: now };
                Some(HotkeyEvent::StartRecording)
            }
            S::MaybeDoubleTap { since } => {
                if now.duration_since(since) <= Duration::from_millis(self.double_tap_window_ms) {
                    self.state = S::Latched;
                    Some(HotkeyEvent::StartLatched)
                } else {
                    // treat as fresh press
                    self.state = S::Recording { since: now };
                    Some(HotkeyEvent::StartRecording)
                }
            }
            S::Latched => {
                self.state = S::Idle;
                Some(HotkeyEvent::StopAndTranscribe)
            }
            S::Recording { .. } => None, // shouldn't happen, already recording
        }
    }

    fn on_chord_release(&mut self, now: Instant) -> Option<HotkeyEvent> {
        if let S::Recording { since } = self.state {
            let dur = now.duration_since(since);
            if dur < Duration::from_millis(self.short_press_ms) {
                self.state = S::MaybeDoubleTap { since: now };
                Some(HotkeyEvent::DiscardRecording)
            } else {
                self.state = S::Idle;
                Some(HotkeyEvent::StopAndTranscribe)
            }
        } else {
            None
        }
    }

    /// Force transition into Latched state (used by hands-free auto-latch).
    /// Chord-release will no longer stop recording; next chord press stops.
    pub fn force_latch(&mut self) {
        if let S::Recording { .. } = self.state {
            self.state = S::Latched;
        }
    }

    /// Force transition to Idle (used after auto-stop so next chord press starts
    /// fresh rather than being interpreted as stop-latched).
    pub fn force_idle(&mut self) {
        self.state = S::Idle;
    }

    /// Called by the timer when the double-tap window elapses.
    pub fn double_tap_expired(&mut self, now: Instant) -> Option<HotkeyEvent> {
        if let S::MaybeDoubleTap { since } = self.state {
            if now.duration_since(since) >= Duration::from_millis(self.double_tap_window_ms) {
                self.state = S::Idle;
                return Some(HotkeyEvent::MaybeDoubleTapExpired);
            }
        }
        None
    }
}
