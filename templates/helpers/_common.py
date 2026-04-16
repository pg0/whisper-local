"""Tiny stdlib-only helpers shared by the .py scripts in this folder."""
import json
import os
import sys
import urllib.request
import urllib.error


def read_stdin() -> str:
    return sys.stdin.read()


def post_json(url: str, body: dict, headers: dict | None = None) -> dict:
    data = json.dumps(body).encode("utf-8")
    merged = {"Content-Type": "application/json; charset=utf-8"}
    if headers:
        merged.update(headers)
    req = urllib.request.Request(url, data=data, headers=merged, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=120) as r:
            return json.loads(r.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        print(f"HTTP {e.code}: {e.read().decode('utf-8', 'replace')}", file=sys.stderr)
        sys.exit(1)


def require_env(name: str) -> str:
    v = os.environ.get(name)
    if not v:
        print(f"environment variable {name} not set", file=sys.stderr)
        sys.exit(1)
    return v
