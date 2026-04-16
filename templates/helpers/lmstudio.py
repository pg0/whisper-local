#!/usr/bin/env python3
"""Pipe the selection to a local LM Studio (OpenAI-compatible) and print reply.

Default: http://localhost:1234/v1/chat/completions.
"""
import os
from _common import read_stdin, post_json

MODEL = os.environ.get("LMSTUDIO_MODEL", "local-model")
URL = os.environ.get("LMSTUDIO_URL", "http://localhost:1234/v1/chat/completions")

user = read_stdin()
r = post_json(URL, {
    "model": MODEL,
    "messages": [{"role": "user", "content": user}],
})
print(r["choices"][0]["message"]["content"], end="")
