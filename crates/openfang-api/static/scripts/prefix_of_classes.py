#!/usr/bin/env python3
"""Prefix OpenFang class names that collide with Bulma (card, modal, tabs, tab)."""
from __future__ import annotations

import re
import sys
from pathlib import Path

# `.tab` must not match inside `.table` / `.tabs` (handled earlier).
_TAB_SELECTOR_RE = re.compile(r"\.tab(?!le)(?!s)(?=[\s:#.\[,>{+~]|$)")

ROOT = Path(__file__).resolve().parents[1]  # static/

# (pattern, replacement) for CSS — longest / most specific first
CSS_REPLACEMENTS: list[tuple[str, str]] = [
    (".tabs.tabs--project-chrome", ".of-tabs.of-tabs--project-chrome"),
    (".tabs--project-chrome", ".of-tabs--project-chrome"),
    (".tab-secondary", ".of-tab-secondary"),
    (".tabs-separator", ".of-tabs-separator"),
    (".tabs", ".of-tabs"),
    (".card-unconfigured", ".of-card-unconfigured"),
    (".card-glow", ".of-card-glow"),
    (".card-flex", ".of-card-flex"),
    (".card-grid", ".of-card-grid"),
    (".card-meta", ".of-card-meta"),
    (".card-header", ".of-card-header"),
    (".card", ".of-card"),
    (".modal-overlay", ".of-modal-overlay"),
    (".modal-header", ".of-modal-header"),
    (".modal-close", ".of-modal-close"),
    (".modal.modal--task-detail", ".of-modal.of-modal--task-detail"),
    (".modal--task-detail", ".of-modal--task-detail"),
    (".modal", ".of-modal"),
]

# Whole class tokens in HTML/JS (class="..." or x-bind:class)
TOKEN_MAP: list[tuple[str, str]] = [
    ("tabs--project-chrome", "of-tabs--project-chrome"),
    ("tab-secondary", "of-tab-secondary"),
    ("tabs-separator", "of-tabs-separator"),
    ("card-unconfigured", "of-card-unconfigured"),
    ("card-glow", "of-card-glow"),
    ("card-flex", "of-card-flex"),
    ("card-grid", "of-card-grid"),
    ("card-meta", "of-card-meta"),
    ("card-header", "of-card-header"),
    ("modal-overlay", "of-modal-overlay"),
    ("modal-header", "of-modal-header"),
    ("modal-close", "of-modal-close"),
    ("modal--task-detail", "of-modal--task-detail"),
    ("tabs", "of-tabs"),
    ("tab", "of-tab"),
    ("card", "of-card"),
    ("modal", "of-modal"),
]


def patch_css(text: str) -> str:
    for old, new in CSS_REPLACEMENTS:
        text = text.replace(old, new)
    text = _TAB_SELECTOR_RE.sub(".of-tab", text)
    return text


def patch_tokens(text: str) -> str:
    def sub_class_attr(m: re.Match[str]) -> str:
        quote = m.group(1)
        inner = m.group(2)
        parts = inner.split()
        out: list[str] = []
        for p in parts:
            replaced = p
            for old, new in TOKEN_MAP:
                if replaced == old:
                    replaced = new
                    break
            out.append(replaced)
        return f"class={quote}{' '.join(out)}{quote}"

    # class="..." or class='...'
    text = re.sub(
        r'class=(["\'])([^"\']*)\1',
        sub_class_attr,
        text,
    )
    return text


def main() -> int:
    exts = {".html", ".js", ".css"}
    for path in sorted(ROOT.rglob("*")):
        if path.suffix.lower() not in exts:
            continue
        if "vendor" in path.parts:
            continue
        if path.name == "prefix_of_classes.py":
            continue
        raw = path.read_text(encoding="utf-8")
        if path.suffix.lower() == ".css":
            new = patch_css(raw)
        else:
            new = patch_tokens(raw)
            if path.suffix.lower() == ".css":
                new = patch_css(new)
        if new != raw:
            path.write_text(new, encoding="utf-8")
            print("updated", path.relative_to(ROOT))
    return 0


if __name__ == "__main__":
    sys.exit(main())
