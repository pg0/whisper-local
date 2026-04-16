use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use regex::Regex;

use crate::config;

/// Cached replace map keyed on the (active filenames, mtimes) signature so
/// the map isn't re-parsed on every transcript chunk unless something
/// actually changed on disk or in the active set.
#[derive(Default)]
pub struct MapCache {
    map: Arc<ReplaceMap>,
    signature: Vec<(String, Option<SystemTime>)>,
}

impl MapCache {
    pub fn get(&mut self, active: &[String]) -> Arc<ReplaceMap> {
        let sig: Vec<_> = active
            .iter()
            .map(|n| (n.clone(), file_mtime(n)))
            .collect();
        if sig != self.signature {
            self.map = Arc::new(load_replace_map(active));
            self.signature = sig;
        }
        self.map.clone()
    }
}

fn file_mtime(name: &str) -> Option<SystemTime> {
    let path = replace_maps_dir()?.join(name);
    fs::metadata(&path).ok().and_then(|m| m.modified().ok())
}

#[derive(Debug, Default)]
pub struct ReplaceMap {
    /// Plain triggers — exact-match (case-insensitive, normalized).
    pub plain: HashMap<String, String>,
    /// Regex rules — applied via `replace_all` to the whole text.
    pub regexes: Vec<(Regex, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Enter,
    Text(String),
    /// Spawn a shell command (passed via `cmd /c`).
    Run(String),
    /// POST the current selection to this URL, replace selection with the
    /// response body.
    Rewrite(String),
    /// Apply a built-in transform to the current selection (lower, upper,
    /// md5, sha256, trim, reverse).
    Transform(String),
    /// Pipe the current selection into an external command via stdin, type
    /// the command's stdout back over the selection.
    Exec(String),
    /// Run an external command with no stdin, type the command's stdout at
    /// the caret. Used when the input comes via captured args rather than a
    /// selection (voice prompts like `/^ask claude (.+)$/`).
    Cmd(String),
    /// Send a sequence of key chords (e.g. `ctrl+a`, `home,shift+end`).
    Keys(String),
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

pub fn replace_maps_dir() -> Option<PathBuf> {
    config::config_dir().ok().map(|d| d.join("replace_maps"))
}

/// Open the file inside `replace_maps/` matching `name`. Used by the tray
/// "Open <map.txt>" menu items.
pub fn replace_map_file(name: &str) -> Option<PathBuf> {
    replace_maps_dir().map(|d| d.join(name))
}

/// All `*.txt` filenames inside `replace_maps/`, sorted alphabetically.
pub fn list_replace_maps() -> Vec<String> {
    let Some(dir) = replace_maps_dir() else {
        return Vec::new();
    };
    let Ok(rd) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("txt"))
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
        .collect();
    names.sort();
    names
}

/// Read `replace_map.txt` from the config dir. Format: one mapping per line,
/// `trigger:replacement`. Blank lines and lines starting with `#` are ignored.
/// Triggers wrapped in slashes — `/pattern/:replacement` — are compiled as
/// regex and applied via `replace_all` to the full chunk; the replacement may
/// reference captures with `$1`, `$2`, etc.
/// Read the named maps in order from `replace_maps/`. Later files override
/// earlier ones (last write wins), so put the more specific map last in the
/// active list.
pub fn load_replace_map(active: &[String]) -> ReplaceMap {
    let mut out = ReplaceMap::default();
    let Some(dir) = replace_maps_dir() else {
        return out;
    };
    for name in active {
        let path = dir.join(name);
        let Ok(s) = fs::read_to_string(&path) else { continue };
        load_into(&s, &mut out);
    }
    out
}

fn load_into(s: &str, out: &mut ReplaceMap) {
    for raw in s.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(idx) = line.find(':') else { continue };
        let key_raw = line[..idx].trim();
        let value = decode_escapes(line[idx + 1..].trim());
        if key_raw.is_empty() {
            continue;
        }
        if let Some(rest) = key_raw.strip_prefix('/') {
            // `/pattern/flags` — flags are JS-style: i, m, s, x.
            let Some(close) = rest.rfind('/') else {
                log::warn!("replace_map regex missing closing `/`: {key_raw}");
                continue;
            };
            let (pat, flags) = (&rest[..close], &rest[close + 1..]);
            if pat.is_empty() {
                continue;
            }
            let inline: String = flags
                .chars()
                .filter(|c| matches!(c, 'i' | 'm' | 's' | 'x'))
                .collect();
            let full = if inline.is_empty() {
                pat.to_string()
            } else {
                format!("(?{inline}){pat}")
            };
            match Regex::new(&full) {
                Ok(re) => out.regexes.push((re, value)),
                Err(e) => log::warn!("replace_map regex `{full}` invalid: {e}"),
            }
        } else {
            out.plain.insert(key_raw.to_lowercase(), value);
        }
    }
}

/// Decode replace-map escape sequences: `\n` → newline, `\t` → tab,
/// `\\` → literal backslash. Unknown escapes pass through verbatim.
fn decode_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Decide what kind of action a plain-trigger value represents based on its
/// prefix: `!` for shell command, `>>` for rewrite-via-URL, otherwise plain
/// text. The prefix is stripped from the returned value.
fn classify_value(value: &str) -> Action {
    if let Some(rest) = value.strip_prefix(">>") {
        let rest = rest.trim();
        if let Some(name) = rest.strip_prefix("local:") {
            Action::Transform(name.trim().to_lowercase())
        } else if let Some(cmd) = rest.strip_prefix("exec:") {
            Action::Exec(cmd.trim().to_string())
        } else if let Some(cmd) = rest.strip_prefix("cmd:") {
            Action::Cmd(cmd.trim().to_string())
        } else {
            Action::Rewrite(rest.to_string())
        }
    } else if let Some(rest) = value.strip_prefix('^') {
        Action::Keys(rest.trim().to_string())
    } else if let Some(rest) = value.strip_prefix('!') {
        Action::Run(rest.trim().to_string())
    } else {
        Action::Text(value.to_string())
    }
}

/// Strip trailing sentence punctuation so "New line." still matches "new line".
fn normalize(text: &str) -> String {
    strip_for_match(text).to_lowercase()
}

/// Same trailing-punctuation trim as `normalize` but case-preserved, used for
/// whole-chunk regex matching so the captured group doesn't include the dot
/// Whisper tacks onto every chunk.
fn strip_for_match(text: &str) -> String {
    text.trim()
        .trim_end_matches(|c: char| matches!(c, '.' | '!' | '?' | ',' | ';' | ':'))
        .trim()
        .to_string()
}

/// Like `process` but returns `None` when no rule fired (so command mode can
/// drop the transcript instead of typing it). Substring regex replacements
/// only count as a "fire" when they actually changed the text.
pub fn process_strict(text: &str, map: &ReplaceMap) -> Option<Action> {
    let key = normalize(text);
    if ENTER_COMMANDS.contains(&key.as_str()) {
        return Some(Action::Enter);
    }
    if let Some(replacement) = map.plain.get(&key) {
        return Some(classify_value(replacement));
    }
    let trimmed = strip_for_match(text);
    for (re, repl) in &map.regexes {
        if let Some(caps) = re.captures(&trimmed) {
            if let Some(m) = caps.get(0) {
                if m.start() == 0 && m.end() == trimmed.len() {
                    let mut buf = String::new();
                    caps.expand(repl, &mut buf);
                    return Some(classify_value(&buf));
                }
            }
        }
    }
    if !map.regexes.is_empty() {
        let mut out = text.to_string();
        let mut changed = false;
        for (re, repl) in &map.regexes {
            let new = re.replace_all(&out, repl.as_str()).into_owned();
            if new != out {
                changed = true;
            }
            out = new;
        }
        if changed {
            return Some(Action::Text(out));
        }
    }
    None
}

pub fn process(text: &str, map: &ReplaceMap) -> Action {
    process_strict(text, map).unwrap_or_else(|| Action::Text(text.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_commands() {
        let map = ReplaceMap::default();
        assert_eq!(process("New line.", &map), Action::Enter);
        assert_eq!(process(" enter ", &map), Action::Enter);
        assert_eq!(process("Neue Zeile!", &map), Action::Enter);
        assert_eq!(process("Hello world.", &map), Action::Text("Hello world.".into()));
    }

    #[test]
    fn plain_replace() {
        let mut map = ReplaceMap::default();
        map.plain.insert("hugging face api key".into(), "hf_xyz".into());
        assert_eq!(process("Hugging Face API key.", &map), Action::Text("hf_xyz".into()));
        assert_eq!(
            process("Hugging Face API key is cool.", &map),
            Action::Text("Hugging Face API key is cool.".into())
        );
    }

    #[test]
    fn regex_replace_substring() {
        let mut map = ReplaceMap::default();
        map.regexes
            .push((Regex::new(r"(?i)\bclode\b").unwrap(), "Claude".into()));
        assert_eq!(
            process("I love clode and Clode.", &map),
            Action::Text("I love Claude and Claude.".into())
        );
    }

    #[test]
    fn run_action_prefix() {
        let mut map = ReplaceMap::default();
        map.plain
            .insert("start battlefield".into(), "!\"C:\\bf.exe\"".into());
        assert_eq!(
            process("start battlefield", &map),
            Action::Run("\"C:\\bf.exe\"".into())
        );
    }

    #[test]
    fn rewrite_action_prefix() {
        let mut map = ReplaceMap::default();
        map.plain
            .insert("fix grammar".into(), ">>https://api.example.com/grammar".into());
        assert_eq!(
            process("fix grammar.", &map),
            Action::Rewrite("https://api.example.com/grammar".into())
        );
    }

    #[test]
    fn regex_whole_match_strips_period() {
        let mut map = ReplaceMap::default();
        map.regexes.push((
            Regex::new(r"(?i)^google for (.+)$").unwrap(),
            "!start \"\" \"https://www.google.com/search?q=$1\"".into(),
        ));
        assert_eq!(
            process("Google for cats.", &map),
            Action::Run("start \"\" \"https://www.google.com/search?q=cats\"".into())
        );
    }

    #[test]
    fn regex_replace_with_capture() {
        let mut map = ReplaceMap::default();
        map.regexes
            .push((Regex::new(r"#(\d+)").unwrap(), "issue #$1".into()));
        assert_eq!(
            process("see #42 later", &map),
            Action::Text("see issue #42 later".into())
        );
    }
}
