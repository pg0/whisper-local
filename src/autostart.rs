use anyhow::Result;
use std::path::Path;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::System::Registry::*;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "WhisperLocal";

pub fn is_enabled() -> bool {
    unsafe {
        let sub = HSTRING::from(RUN_KEY);
        let name = HSTRING::from(VALUE_NAME);
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sub.as_ptr()), 0, KEY_READ, &mut hkey).is_err() {
            return false;
        }
        let mut kind = REG_VALUE_TYPE(0);
        let mut size = 0u32;
        let exists = RegQueryValueExW(
            hkey,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            None,
            Some(&mut size),
        )
        .is_ok();
        let _ = RegCloseKey(hkey);
        exists
    }
}

pub fn set_enabled(enabled: bool, exe_path: &Path) -> Result<()> {
    unsafe {
        let sub = HSTRING::from(RUN_KEY);
        let name = HSTRING::from(VALUE_NAME);
        let mut hkey = HKEY::default();
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        )
        .ok()?;
        if enabled {
            let p = format!("\"{}\"", exe_path.display());
            let wide: Vec<u16> = p.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes =
                std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            RegSetValueExW(hkey, PCWSTR(name.as_ptr()), 0, REG_SZ, Some(bytes)).ok()?;
        } else {
            let _ = RegDeleteValueW(hkey, PCWSTR(name.as_ptr()));
        }
        let _ = RegCloseKey(hkey);
        Ok(())
    }
}

pub fn current_exe_path() -> Result<std::path::PathBuf> {
    Ok(std::env::current_exe()?)
}
