#!/usr/bin/env python3
"""Pipe the selection into the `claude` CLI and print its reply.

Invoke from a replace_map entry like:
    ask claude:>>exec:python "%APPDATA%\\whisper-local\\helpers\\claude.py"

The CLI must be on PATH. Claude Code:
    https://github.com/anthropics/claude-code
"""
import subprocess
import sys

PROMPT = "Reply concisely."
ALLOWED_TOOLS = "Read,Edit,Bash"

selection = sys.stdin.read()
full = f"{PROMPT}\n\n{selection}"

r = subprocess.run(
    ["claude", "-p", "--allowedTools", ALLOWED_TOOLS, "-"],
    input=full,
    text=True,
    capture_output=True,
)
sys.stdout.write(r.stdout)
if r.returncode != 0:
    sys.stderr.write(r.stderr)
    sys.exit(r.returncode)
