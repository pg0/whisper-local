use std::thread;
use std::time::Duration;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub fn press_enter() {
    let down = vk_input(VK_RETURN, false);
    let up = vk_input(VK_RETURN, true);
    unsafe {
        SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32);
    }
    thread::sleep(Duration::from_millis(1));
}

/// Press Ctrl+C so the active window copies the current selection to the
/// clipboard. The caller should give Windows a moment afterwards before
/// reading the clipboard.
pub fn send_copy() {
    send_chord(&["ctrl", "c"]);
}

/// Send a sequence of key chords. `seq` is comma-separated; each entry is `+`-
/// separated tokens (modifier(s) + key). e.g. `ctrl+a`, `home,shift+end`.
pub fn send_keys(seq: &str) {
    for chord in seq.split(',') {
        let parts: Vec<&str> = chord.split('+').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            continue;
        }
        send_chord(&parts);
        thread::sleep(Duration::from_millis(5));
    }
}

fn send_chord(parts: &[&str]) {
    let mut downs: Vec<INPUT> = Vec::new();
    let mut ups: Vec<INPUT> = Vec::new();
    for token in parts {
        let Some(vk) = vk_for_token(token) else {
            log::warn!("send_keys: unknown key token `{token}`");
            return;
        };
        downs.push(vk_input(vk, false));
        ups.push(vk_input(vk, true));
    }
    ups.reverse();
    let mut all = downs;
    all.extend(ups);
    unsafe {
        SendInput(&all, std::mem::size_of::<INPUT>() as i32);
    }
}

fn vk_for_token(token: &str) -> Option<VIRTUAL_KEY> {
    let t = token.to_lowercase();
    Some(match t.as_str() {
        "ctrl" | "control" => VK_CONTROL,
        "shift" => VK_SHIFT,
        "alt" => VK_MENU,
        "win" | "lwin" => VK_LWIN,
        "rwin" => VK_RWIN,
        "enter" | "return" => VK_RETURN,
        "tab" => VK_TAB,
        "esc" | "escape" => VK_ESCAPE,
        "space" => VK_SPACE,
        "backspace" | "bksp" => VK_BACK,
        "delete" | "del" => VK_DELETE,
        "home" => VK_HOME,
        "end" => VK_END,
        "pageup" | "pgup" => VK_PRIOR,
        "pagedown" | "pgdn" => VK_NEXT,
        "insert" | "ins" => VK_INSERT,
        "printscreen" | "prtsc" | "print" => VK_SNAPSHOT,
        "pause" | "break" => VK_PAUSE,
        "capslock" => VK_CAPITAL,
        "scrolllock" => VK_SCROLL,
        "numlock" => VK_NUMLOCK,
        "menu" | "apps" => VK_APPS,
        "left" => VK_LEFT,
        "right" => VK_RIGHT,
        "up" => VK_UP,
        "down" => VK_DOWN,
        "f1" => VK_F1, "f2" => VK_F2, "f3" => VK_F3, "f4" => VK_F4,
        "f5" => VK_F5, "f6" => VK_F6, "f7" => VK_F7, "f8" => VK_F8,
        "f9" => VK_F9, "f10" => VK_F10, "f11" => VK_F11, "f12" => VK_F12,
        s if s.len() == 1 => {
            let c = s.chars().next().unwrap();
            if c.is_ascii_alphabetic() {
                VIRTUAL_KEY(c.to_ascii_uppercase() as u16)
            } else if c.is_ascii_digit() {
                VIRTUAL_KEY(c as u16)
            } else {
                return None;
            }
        }
        _ => return None,
    })
}

fn vk_input(vk: VIRTUAL_KEY, key_up: bool) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if key_up { flags |= KEYEVENTF_KEYUP; }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

pub fn type_text(s: &str) {
    if s.is_empty() { return; }
    // Split on '\n' so newlines map to a real Enter keypress (most apps treat
    // unicode 0x0A as plain text, not a line break). '\r' is dropped.
    let cleaned: String = s.chars().filter(|c| *c != '\r').collect();
    let mut first = true;
    for segment in cleaned.split('\n') {
        if !first {
            press_enter();
        }
        first = false;
        type_unicode(segment);
    }
}

fn type_unicode(s: &str) {
    if s.is_empty() { return; }
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut inputs: Vec<INPUT> = Vec::with_capacity(units.len() * 2);
    for &u in &units {
        inputs.push(unicode_input(u, false));
        inputs.push(unicode_input(u, true));
    }
    const CHUNK: usize = 40;
    for batch in inputs.chunks(CHUNK) {
        unsafe {
            let n = SendInput(batch, std::mem::size_of::<INPUT>() as i32);
            if (n as usize) != batch.len() {
                log::warn!("SendInput dropped events: sent {}/{}", n, batch.len());
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn unicode_input(ch: u16, key_up: bool) -> INPUT {
    let mut flags = KEYEVENTF_UNICODE;
    if key_up { flags |= KEYEVENTF_KEYUP; }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: ch,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
