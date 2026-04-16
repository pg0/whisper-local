#!/usr/bin/env python3
"""Pipe the selection to OpenAI chat completions and print the assistant reply.

Needs OPENAI_API_KEY in the environment. Set once per terminal:
    $env:OPENAI_API_KEY = 'sk-...'        (PowerShell)
    set OPENAI_API_KEY=sk-...              (cmd.exe)
Persist for your user:
    setx OPENAI_API_KEY "sk-..."
"""
from _common import read_stdin, post_json, require_env

MODEL = "gpt-4o-mini"
SYSTEM = "Reply concisely. Return just the rewritten text."

api_key = require_env("OPENAI_API_KEY")
user = read_stdin()

r = post_json(
    "https://api.openai.com/v1/chat/completions",
    {
        "model": MODEL,
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": user},
        ],
    },
    headers={"Authorization": f"Bearer {api_key}"},
)
print(r["choices"][0]["message"]["content"], end="")
