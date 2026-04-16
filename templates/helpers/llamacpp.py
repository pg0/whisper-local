#!/usr/bin/env python3
"""Pipe the selection to a local llama.cpp server and print the reply.

Default: http://localhost:8080/v1/chat/completions.
Start with:
    llama-server -m model.gguf --port 8080
"""
import os
from _common import read_stdin, post_json

MODEL = os.environ.get("LLAMACPP_MODEL", "local-model")
URL = os.environ.get("LLAMACPP_URL", "http://localhost:8080/v1/chat/completions")

user = read_stdin()
r = post_json(URL, {
    "model": MODEL,
    "messages": [{"role": "user", "content": user}],
})
print(r["choices"][0]["message"]["content"], end="")
