use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Enter,
    Text(String),
}

/// Whisper-chunk commands that mean "press Enter" when spoken alone.
const ENTER_COMMANDS: &[&str] = &[
    "new line",
    "newline",
    "enter",
    "return",
    "neue zeile",
    "zeilenumbruch",
    "absatz",
];

pub fn replace_map_path() -> Option<PathBuf> {
    config::config_dir().ok().map(|d| d.join("replace_map.txt"))
}

/// Read `replace_map.txt` from the config dir. Format: one mapping per line,
/// `trigger:replacement`. Blank lines and lines starting with `#` are ignored.
/// The replacement keeps everything after the first `:` verbatim, so keys may
/// not contain `:` but values may.
pub fn load_replace_map() -> HashMap<String, String> {
    let Some(path) = replace_map_path() else {
        return HashMap::new();
    };
    let Ok(s) = fs::read_to_string(&path) else {
        return HashMap::new();
    };
    let mut map = HashMap::new();
    for raw in s.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(idx) = line.find(':') else { continue };
        let key = line[..idx].trim().to_lowercase();
        let value = line[idx + 1..].trim().to_string();
        if !key.is_empty() {
            map.insert(key, value);
        }
    }
    map
}

/// Strip trailing sentence punctuation so "New line." still matches "new line".
fn normalize(text: &str) -> String {
    text.trim()
        .trim_end_matches(|c: char| matches!(c, '.' | '!' | '?' | ',' | ';' | ':'))
        .trim()
        .to_lowercase()
}

pub fn process(text: &str, replace_map: &HashMap<String, String>) -> Action {
    let key = normalize(text);
    if ENTER_COMMANDS.contains(&key.as_str()) {
        return Action::Enter;
    }
    if let Some(replacement) = replace_map.get(&key) {
        return Action::Text(replacement.clone());
    }
    Action::Text(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_commands() {
        let map = HashMap::new();
        assert_eq!(process("New line.", &map), Action::Enter);
        assert_eq!(process(" enter ", &map), Action::Enter);
        assert_eq!(process("Neue Zeile!", &map), Action::Enter);
        assert_eq!(process("Hello world.", &map), Action::Text("Hello world.".into()));
    }

    #[test]
    fn replace_map_hits() {
        let mut map = HashMap::new();
        map.insert("hugging face api key".into(), "hf_xyz".into());
        assert_eq!(process("Hugging Face API key.", &map), Action::Text("hf_xyz".into()));
        assert_eq!(process("Hugging Face API key is cool.", &map), Action::Text("Hugging Face API key is cool.".into()));
    }
}
