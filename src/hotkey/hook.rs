use crate::hotkey::state::{HotkeyEvent, KeyEvent, Machine, VKey};
use crossbeam_channel::Sender;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

static SHARED: OnceCell<Shared> = OnceCell::new();

struct Shared {
    machine: Mutex<Machine>,
    tx: Sender<HotkeyEvent>,
    timer_tx: Sender<Instant>,
}

fn vk_map(vk: u32) -> VKey {
    match vk {
        x if x == u32::from(VK_LCONTROL.0) => VKey::LCtrl,
        x if x == u32::from(VK_RCONTROL.0) => VKey::RCtrl,
        x if x == u32::from(VK_LWIN.0) => VKey::LWin,
        x if x == u32::from(VK_RWIN.0) => VKey::RWin,
        _ => VKey::Other,
    }
}

unsafe extern "system" fn ll_keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kbd = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let is_down =
            wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN;
        let is_up =
            wparam.0 as u32 == WM_KEYUP || wparam.0 as u32 == WM_SYSKEYUP;
        if is_down || is_up {
            let v = vk_map(kbd.vkCode);
            let now = Instant::now();
            let ev = if is_down {
                KeyEvent::Down(v, now)
            } else {
                KeyEvent::Up(v, now)
            };
            if let Some(s) = SHARED.get() {
                let emitted = { s.machine.lock().on(ev) };
                if let Some(he) = emitted {
                    let _ = s.tx.send(he);
                    if matches!(he, HotkeyEvent::DiscardRecording) {
                        let _ = s.timer_tx.send(now);
                    }
                }
            }
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

/// Hands-free: force state machine into Latched so chord-release doesn't stop.
pub fn force_latch() {
    if let Some(s) = SHARED.get() {
        s.machine.lock().force_latch();
    }
}

/// Reset state machine to Idle (called after auto-stop fires).
pub fn force_idle() {
    if let Some(s) = SHARED.get() {
        s.machine.lock().force_idle();
    }
}

pub fn spawn_hook(tx: Sender<HotkeyEvent>) -> anyhow::Result<()> {
    let (timer_tx, timer_rx) = crossbeam_channel::bounded::<Instant>(8);
    let shared = Shared {
        machine: Mutex::new(Machine::default()),
        tx: tx.clone(),
        timer_tx,
    };
    SHARED
        .set(shared)
        .ok()
        .ok_or_else(|| anyhow::anyhow!("hook already installed"))?;

    // Timer thread: waits for DiscardRecording events and fires double-tap-expired
    {
        let tx2 = tx;
        thread::spawn(move || {
            while let Ok(armed_at) = timer_rx.recv() {
                let mut latest = armed_at;
                while let Ok(t) = timer_rx.try_recv() {
                    latest = t;
                }
                thread::sleep(Duration::from_millis(420));
                if let Some(s) = SHARED.get() {
                    let emitted = {
                        s.machine
                            .lock()
                            .double_tap_expired(latest + Duration::from_millis(420))
                    };
                    if let Some(he) = emitted {
                        let _ = tx2.send(he);
                    }
                }
            }
        });
    }

    // Hook + message pump thread
    thread::spawn(|| unsafe {
        let hmod = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
            .unwrap_or_default();
        // SetWindowsHookExW expects HINSTANCE; HMODULE and HINSTANCE have same repr
        let hinstance = HINSTANCE(hmod.0);
        let hhook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard_proc), hinstance, 0)
            .expect("SetWindowsHookExW failed");
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(hhook);
    });

    Ok(())
}
