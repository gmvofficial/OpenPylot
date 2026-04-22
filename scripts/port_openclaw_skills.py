#!/usr/bin/env python3
"""Port OpenClaw skills (extra_repos/openclaw-main/skills/*) to our flat SKILL.md schema.

OpenClaw frontmatter uses nested `metadata.openclaw.*` with JSON-ish YAML;
our loader expects flat fields: name, description, version, category, tags,
os, requires (bins/env/tools), install (list of installers).

Usage:
    python3 scripts/port_openclaw_skills.py

Writes into: skills/<category>/<skill-name>/SKILL.md
Skips macOS-only skills that have no Linux alternative, unless opted in.
"""
from __future__ import annotations
import re
import sys
import yaml
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "extra_repos" / "openclaw-main" / "skills"
DST = ROOT / "skills"

# Curated: (openclaw_name, target_category, override_name_or_None, os_filter_list)
# os_filter_list empty = universal; otherwise restrict.
CURATED = [
    # Productivity
    ("notion",             "productivity",   None, []),
    ("obsidian",           "productivity",   None, []),
    ("trello",             "productivity",   None, []),
    ("summarize",          "productivity",   None, []),
    ("things-mac",         "productivity",   "things-mac", ["macos"]),
    ("apple-notes",        "productivity",   None, ["macos"]),
    ("apple-reminders",    "productivity",   None, ["macos"]),
    ("bear-notes",         "productivity",   None, ["macos"]),

    # Coding
    ("github",             "coding",         None, []),
    ("coding-agent",       "coding",         None, []),
    ("openai-image-gen",   "coding",         None, []),
    ("nano-pdf",           "coding",         None, []),
    ("skill-creator",      "coding",         None, []),
    ("openai-whisper",     "coding",         None, []),
    ("openai-whisper-api", "coding",         None, []),

    # Communication
    ("discord",            "communication",  None, []),
    ("slack",              "communication",  None, []),
    ("imsg",               "communication",  None, ["macos"]),

    # Research
    ("weather",            "research",       None, []),
    ("goplaces",           "research",       None, []),
    ("blogwatcher",        "research",       None, []),

    # Media (new category)
    ("spotify-player",     "media",          None, []),
    ("video-frames",       "media",          None, []),
    ("gifgrep",            "media",          None, []),
    ("songsee",            "media",          None, ["macos"]),

    # System (new category)
    ("tmux",               "system",         None, ["macos", "linux"]),
    ("healthcheck",        "system",         None, []),
    ("openhue",            "system",         None, []),
    ("1password",          "system",         "onepassword", []),
]

FRONTMATTER_RE = re.compile(r"^---\s*\n(.*?)\n---\s*\n?(.*)$", re.DOTALL)


def parse_openclaw(md_text: str) -> tuple[dict, str]:
    m = FRONTMATTER_RE.match(md_text)
    if not m:
        raise ValueError("No YAML frontmatter")
    raw = m.group(1)
    body = m.group(2)
    # Try to parse as YAML. OpenClaw mixes YAML + JSON-in-YAML which pyyaml handles.
    data = yaml.safe_load(raw)
    if not isinstance(data, dict):
        raise ValueError(f"Frontmatter not a dict: {type(data)}")
    return data, body


def convert(meta: dict, fallback_name: str, category: str, os_filter: list[str]) -> dict:
    """Convert OpenClaw frontmatter dict to our flat schema."""
    out = {
        "name": meta.get("name", fallback_name),
        "description": (meta.get("description") or "").strip(),
        "version": "1.0.0",
        "category": category,
        "tags": [],
    }
    if meta.get("author"):
        out["author"] = meta["author"]

    # Extract nested metadata.openclaw
    oc = ((meta.get("metadata") or {}).get("openclaw") or {})

    # OS filter — prefer explicit curated filter, else pull from OpenClaw
    if os_filter:
        out["os"] = os_filter
    elif oc.get("os"):
        # OpenClaw uses "darwin" not "macos"
        mapped = ["macos" if x == "darwin" else x for x in oc["os"]]
        out["os"] = mapped

    # Requirements
    req = oc.get("requires") or {}
    if req.get("bins") or req.get("env") or req.get("tools"):
        out["requires"] = {}
        if req.get("bins"):
            out["requires"]["bins"] = req["bins"]
        if req.get("env"):
            out["requires"]["env"] = req["env"]
        if req.get("tools"):
            out["requires"]["tools"] = req["tools"]

    # Installers
    installs = oc.get("install") or []
    converted = []
    for inst in installs:
        if not isinstance(inst, dict):
            continue
        item = {
            "id": inst.get("id", "brew"),
            "kind": inst.get("kind", "brew"),
        }
        if "formula" in inst:
            item["formula"] = inst["formula"]
        if "package" in inst:
            item["package"] = inst["package"]
        if "bins" in inst:
            item["bins"] = inst["bins"]
        if "label" in inst:
            item["label"] = inst["label"]
        converted.append(item)
    if converted:
        out["install"] = converted

    # Keep homepage as a tag for discoverability if present
    if meta.get("homepage"):
        out.setdefault("tags", []).append(meta["homepage"])

    return out


def dump_yaml(d: dict) -> str:
    # Use block style, preserve order
    return yaml.dump(d, sort_keys=False, default_flow_style=False, allow_unicode=True)


def port_one(src_name: str, category: str, override_name: str | None, os_filter: list[str]) -> str:
    src_path = SRC / src_name / "SKILL.md"
    if not src_path.exists():
        return f"SKIP (missing): {src_name}"
    try:
        text = src_path.read_text(encoding="utf-8")
        meta, body = parse_openclaw(text)
    except Exception as e:
        return f"ERROR parsing {src_name}: {e}"

    name = override_name or meta.get("name") or src_name
    converted = convert(meta, fallback_name=name, category=category, os_filter=os_filter)

    dst_dir = DST / category / name
    dst_dir.mkdir(parents=True, exist_ok=True)
    dst_path = dst_dir / "SKILL.md"

    # Rewrite body references from "openclaw" -> "pylot" in prose (gentle)
    cleaned_body = body.strip()
    # Don't rewrite inside code blocks aggressively; simple prose replacements only.
    # We only rewrite the first heading if it says "OpenClaw".
    cleaned_body = re.sub(r"(?i)\bopenclaw\b", "pylot", cleaned_body)

    out_text = "---\n" + dump_yaml(converted) + "---\n\n" + cleaned_body + "\n"
    dst_path.write_text(out_text, encoding="utf-8")
    return f"OK: {name} -> {dst_path.relative_to(ROOT)}"


def main() -> int:
    if not SRC.exists():
        print(f"Source not found: {SRC}", file=sys.stderr)
        return 1
    results = []
    for name, cat, override, os_filter in CURATED:
        results.append(port_one(name, cat, override, os_filter))
    print("\n".join(results))
    ok = sum(1 for r in results if r.startswith("OK"))
    skip = sum(1 for r in results if r.startswith("SKIP"))
    err = sum(1 for r in results if r.startswith("ERROR"))
    print(f"\nPorted: {ok} ok, {skip} skipped, {err} errors")
    return 0 if err == 0 else 2


if __name__ == "__main__":
    sys.exit(main())
