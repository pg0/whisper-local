# whisper-local — Replace-map syntax

Files live under `%APPDATA%\whisper-local\replace_maps\*.txt`.
One mapping per line. `#` starts a comment. Blank lines are ignored.

```
trigger:replacement
```

Triggers are matched against each whole transcribed chunk —
case-insensitive, trailing `.`/`!`/`?`/`,`/`;`/`:` stripped.

---

## Trigger forms

```
plain text                Match the whole chunk verbatim.
/pattern/                 Regex (substring substitution).
/pattern/flags            Regex with flags.  i  m  s  x
/^pattern$/flags          Whole-chunk regex — value is run as an action,
                          captures expand via $1, $2, ...
```

## Value prefixes

```
(none)                    Type the value as text.
!cmd args                 Run shell command via `cmd /c`.
>>https://url             POST current selection, replace with response.
>>local:NAME              Apply built-in transform to the selection.
^chord[,chord ...]        Send key sequence (modifiers+key, comma-sequenced).
```

## Built-in transforms (`>>local:`)

```
lower      uppercase → lowercase
upper      lowercase → uppercase
trim       strip leading + trailing whitespace
reverse    reverse character order
md5        hex MD5 hash
sha256     hex SHA-256 hash
```

## Key chord tokens (`^`)

```
ctrl shift alt win        Modifiers (also: control, lwin, rwin)
a-z 0-9                   Letters and digits
enter return tab esc      Symbolic keys
space backspace delete    Symbolic keys
home end pageup pagedown  Symbolic keys
left right up down        Symbolic keys
f1-f12                    Function keys

Chords combine with +     ctrl+shift+a
Sequences split on comma  home,shift+end,delete
```

## Escape sequences (in the replacement value)

```
\n         newline (presses Enter between segments)
\t         tab character
\\         literal backslash
\<other>   passes through verbatim
```

## Built-in voice commands → Enter key

```
new line   newline   enter   return
neue zeile zeilenumbruch     absatz
```

(Match the whole chunk; trailing punctuation tolerated.)

---

## Examples

```
# plain text
my email:email@example.com

# multi-line text
my signature:--\nName\nemail@example.com

# regex substring (whisper mishearings)
/\bclode\b/i:Claude

# regex with parameter, runs a shell command
/^google for (.+)$/i:!start "" "https://www.google.com/search?q=$1"

# launch programs / shortcuts
open browser:!start chrome
open settings:^win+i

# selection transforms
lowercase selected:>>local:lower
md5 selected:>>local:md5

# rewrite selection via remote service
fix grammar:>>https://api.example.com/grammar
```

---

## Active maps

`config.toml` → `enabled_replace_maps = ["global.txt", "launch.txt"]`.

Toggle in tray → **Replace maps** submenu. Files in `replace_maps/` are
listed; check the ones you want active. Files load in order — later
files override earlier entries (last write wins).

Master kill-switch: tray **Replace maps → Enabled** off ⇒ no map fires.

---

## Regex flavour

The `regex` crate (Rust). RE2-style — fast, no backtracking.
Common cheat-sheet:

```
a|b              a or b
.                any single char
[abc] [^abc]     character class / negated
[a-z]            range
^  $             chunk boundaries (with whole-match form)
( )              capture group
\b               word boundary
\d \w \s         digit / word / whitespace shortcuts
*  +  ?          0+ / 1+ / 0-1
*?  +?  ??       lazy variants
{n} {n,} {n,m}   exact / min / range repetitions
\1 \2 ...        backreferences inside the pattern
$1 $2 ...        backreferences inside the replacement value
\\               literal backslash in the pattern
(?i) (?m) ...    inline flags (or use /pattern/i)
```

Whisper-local always anchors a *whole-chunk* match by checking
`match.start() == 0 && match.end() == chunk.len()`. To get parameter
binding, write `^...$` explicitly.
