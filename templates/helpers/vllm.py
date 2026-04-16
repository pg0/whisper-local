#!/usr/bin/env python3
"""Pipe the selection to a local vLLM (OpenAI-compatible) and print reply.

Default: http://localhost:8000/v1/chat/completions.
Start vLLM with:
    python -m vllm.entrypoints.openai.api_server --model <repo>
"""
import os
from _common import read_stdin, post_json

MODEL = os.environ.get("VLLM_MODEL", "local-model")
URL = os.environ.get("VLLM_URL", "http://localhost:8000/v1/chat/completions")

user = read_stdin()
r = post_json(URL, {
    "model": MODEL,
    "messages": [{"role": "user", "content": user}],
})
print(r["choices"][0]["message"]["content"], end="")
