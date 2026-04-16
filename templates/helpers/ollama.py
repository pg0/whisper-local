#!/usr/bin/env python3
"""Pipe the selection to a local Ollama chat endpoint and print the reply.

Default: http://localhost:11434/api/chat (set OLLAMA_MODEL to override model).
"""
import os
from _common import read_stdin, post_json

MODEL = os.environ.get("OLLAMA_MODEL", "llama3")
URL = os.environ.get("OLLAMA_URL", "http://localhost:11434/api/chat")

user = read_stdin()
r = post_json(URL, {
    "model": MODEL,
    "stream": False,
    "messages": [{"role": "user", "content": user}],
})
print(r["message"]["content"], end="")
