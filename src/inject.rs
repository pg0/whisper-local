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
