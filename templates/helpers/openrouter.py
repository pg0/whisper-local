#!/usr/bin/env python3
"""Pipe the selection to OpenRouter (OpenAI-compatible) and print reply.

Needs OPENROUTER_API_KEY in the environment.
Models use the provider/model format:
    anthropic/claude-3.5-sonnet
    openai/gpt-4o-mini
    meta-llama/llama-3.1-70b-instruct
"""
import os
from _common import read_stdin, post_json, require_env

MODEL = os.environ.get("OPENROUTER_MODEL", "openai/gpt-4o-mini")
SYSTEM = "Reply concisely. Return just the rewritten text."

api_key = require_env("OPENROUTER_API_KEY")
user = read_stdin()

r = post_json(
    "https://openrouter.ai/api/v1/chat/completions",
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
