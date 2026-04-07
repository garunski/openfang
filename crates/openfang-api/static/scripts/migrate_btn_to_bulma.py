#!/usr/bin/env python3
"""Map OpenFang .btn* classes to Bulma .button + .is-* (and OF-specific modifiers)."""
from __future__ import annotations

import re
import sys
from pathlib import Path

STATIC = Path(__file__).resolve().parents[1]

# Whole class tokens, longest match wins via ordered iteration
TOKEN_ORDER = [
    ("btn-send", "of-chat-send"),
    ("btn-stop", "of-chat-stop"),
    ("btn-recording", "of-voice-recording"),
    ("btn-launch", "of-hand-launch"),
    ("btn-block", "is-fullwidth"),
    ("btn-sm", "is-small"),
    ("btn-primary", "is-primary"),
    ("btn-danger", "is-danger"),
    ("btn-success", "is-success"),
    ("btn-ghost", "is-ghost"),
    ("btn", "button"),
]
TOKEN_MAP = dict(TOKEN_ORDER)


def migrate_class_attr_value(value: str) -> str:
    parts = value.split()
    out: list[str] = []
    for p in parts:
        out.append(TOKEN_MAP.get(p, p))
    return " ".join(out)


def patch_html(text: str) -> str:
    def repl(m: re.Match[str]) -> str:
        q = m.group(1)
        inner = migrate_class_attr_value(m.group(2))
        return f"class={q}{inner}{q}"

    return re.sub(r'class=(["\'])([^"\']*)\1', repl, text)


def main() -> int:
    paths = [STATIC / "index_body.html", STATIC / "js" / "api.js"]
    for path in paths:
        if not path.exists():
            print("skip missing", path)
            continue
        raw = path.read_text(encoding="utf-8")
        new = patch_html(raw) if path.suffix == ".html" else raw
        if path.name == "api.js":
            new = new.replace("'btn btn-ghost confirm-cancel'", "'button is-ghost confirm-cancel'")
            new = new.replace("'btn btn-danger confirm-ok'", "'button is-danger confirm-ok'")
        if new != raw:
            path.write_text(new, encoding="utf-8")
            print("updated", path.relative_to(STATIC))
    return 0


if __name__ == "__main__":
    sys.exit(main())
