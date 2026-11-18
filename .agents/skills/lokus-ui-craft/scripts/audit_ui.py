#!/usr/bin/env python3
"""Static UI anti-pattern auditor for Lokus Judicial (Svelte 5 + CSS Variables).

Scans .svelte, .css, and .html files for deterministic "AI slop" and safety
anti-patterns without requiring external packages or interactive input.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter
from pathlib import Path

IGNORED_DIRS = {
    "node_modules",
    "dist",
    "build",
    ".git",
    "target",
    ".svelte-kit",
    ".agents",
    "__pycache__",
}

TARGET_EXTENSIONS = {".svelte", ".css", ".html"}

# Unicode emoji & symbol ranges commonly misused as UI icons
EMOJI_PATTERN = re.compile(
    "["
    "\U0001F300-\U0001F5FF"  # Misc Symbols & Pictographs
    "\U0001F600-\U0001F64F"  # Emoticons
    "\U0001F680-\U0001F6FF"  # Transport & Map
    "\U0001F900-\U0001F9FF"  # Supplemental Symbols
    "\U0001FA70-\U0001FAFF"  # Symbols Extended-A
    "\u2600-\u26FF"          # Misc Symbols
    "\u2700-\u27BF"          # Dingbats
    "]"
)

CSS_COLOR_PROP_RE = re.compile(
    r"^\s*(?!--)([a-z-]+)\s*:\s*([^;{}]+);",
    re.IGNORECASE | re.MULTILINE,
)
COLOR_PROPS_EXACT = {
    "background",
    "fill",
    "stroke",
    "outline",
    "border",
    "border-top",
    "border-right",
    "border-bottom",
    "border-left",
}
RAW_COLOR_VALUE_RE = re.compile(
    r"(#[0-9a-fA-F]{3,8}\b|\brgba?\s*\(|\bhsla?\s*\()",
    re.IGNORECASE,
)

BORDER_RADIUS_PROP_RE = re.compile(
    r"^\s*(?!--)border(?:-(?:top|bottom)-(?:left|right))?-radius\s*:\s*([^;{}]+);",
    re.IGNORECASE | re.MULTILINE,
)

CSS_BLOCK_RE = re.compile(r"([^{}]+)\{([^{}]+)\}")
STYLE_TAG_RE = re.compile(r"<style[^>]*>(.*?)</style>", re.IGNORECASE | re.DOTALL)
SCRIPT_TAG_RE = re.compile(r"<script[^>]*>(.*?)</script>", re.IGNORECASE | re.DOTALL)
HTML_COMMENT_RE = re.compile(r"<!--.*?-->", re.DOTALL)
CSS_COMMENT_RE = re.compile(r"/\*.*?\*/", re.DOTALL)
HEADING_TAG_RE = re.compile(r"<h([1-6])\b", re.IGNORECASE)
SECRET_LOG_RE = re.compile(
    r"console\.(?:log|info|warn|debug)\s*\([^\n]*(?:api[_-]?key|secret|password|bearer|openai[_-]?key|gemini[_-]?key|\.env)",
    re.IGNORECASE,
)
TAILWIND_CLASS_SMELL_RE = re.compile(
    r'class(?:Name)?=["\'][^"\']*\b(?:flex|grid|px-\d|py-\d|bg-(?:slate|gray|zinc|blue|red)-\d{2,3}|rounded-(?:md|lg|xl)|shadow-(?:sm|md|lg))\b[^"\']*["\']'
)


def line_number_at(text: str, index: int) -> int:
    return text.count("\n", 0, index) + 1


def extract_css_segments(path: Path, text: str) -> list[tuple[str, int]]:
    """Return list of (css_text, start_line_offset) for .css or <style> blocks."""
    if path.suffix == ".css":
        return [(text, 0)]
    segments: list[tuple[str, int]] = []
    for match in STYLE_TAG_RE.finditer(text):
        offset = text.count("\n", 0, match.start(1))
        segments.append((match.group(1), offset))
    return segments


def strip_non_markup(text: str) -> str:
    """Replace script, style, and comment blocks with newlines preserving line numbers."""
    def _blank(match: re.Match[str]) -> str:
        return "\n" * match.group(0).count("\n")

    cleaned = SCRIPT_TAG_RE.sub(_blank, text)
    cleaned = STYLE_TAG_RE.sub(_blank, cleaned)
    cleaned = HTML_COMMENT_RE.sub(_blank, cleaned)
    return cleaned


def is_border_active(val: str) -> bool:
    v = val.strip().lower()
    if v in {"none", "0", "0px", "transparent", "inherit", "unset", "initial"}:
        return False
    if re.match(r"^0(?:px|rem|em)?\b", v):
        return False
    return True


def is_shadow_active(val: str) -> bool:
    v = val.strip().lower()
    return v not in {"none", "inherit", "unset", "initial", "0"}


def audit_file(path: Path, root: Path) -> list[dict]:
    issues: list[dict] = []
    rel_path = str(path.relative_to(root))
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        print(f"[warn] Skipping unreadable file {rel_path}: {exc}", file=sys.stderr)
        return issues

    # 1. CSS checks (.css and <style> inside .svelte/.html)
    for css_text, line_offset in extract_css_segments(path, text):
        css_clean = CSS_COMMENT_RE.sub(lambda m: "\n" * m.group(0).count("\n"), css_text)

        # Hardcoded colors outside custom property declarations
        for m in CSS_COLOR_PROP_RE.finditer(css_clean):
            prop_name = m.group(1).lower()
            prop_val = m.group(2)
            val_without_vars = re.sub(r"var\([^)]*\)", "", prop_val)
            if ("color" in prop_name or prop_name in COLOR_PROPS_EXACT) and RAW_COLOR_VALUE_RE.search(val_without_vars):
                issues.append(
                    {
                        "rule": "hardcoded-color",
                        "severity": "warning",
                        "file": rel_path,
                        "line": line_offset + line_number_at(css_clean, m.start()),
                        "snippet": m.group(0).strip(),
                        "message": "Replace raw color literal with a CSS variable (var(--...)) defined in DESIGN.md.",
                    }
                )

        # Arbitrary border-radius outside var(--...)
        for m in BORDER_RADIUS_PROP_RE.finditer(css_clean):
            val = m.group(1).strip().lower()
            if "var(" in val:
                continue
            allowed_tokens = {"0", "0px", "50%", "9999px", "inherit", "initial", "unset"}
            parts = val.split()
            if not all(p in allowed_tokens for p in parts):
                issues.append(
                    {
                        "rule": "arbitrary-radius",
                        "severity": "warning",
                        "file": rel_path,
                        "line": line_offset + line_number_at(css_clean, m.start()),
                        "snippet": m.group(0).strip(),
                        "message": "Use a DESIGN.md radius token (var(--radius-sm/md/lg/full)) instead of an arbitrary value.",
                    }
                )

        # Border + box-shadow combined in the same CSS rule block
        for block in CSS_BLOCK_RE.finditer(css_clean):
            selector = block.group(1).strip()
            body = block.group(2)
            if selector.startswith("@"):
                continue
            has_border = False
            has_shadow = False
            for decl in re.finditer(r"^\s*([a-z-]+)\s*:\s*([^;]+);", body, re.IGNORECASE | re.MULTILINE):
                prop = decl.group(1).lower()
                val = decl.group(2)
                if prop in {"border", "border-top", "border-right", "border-bottom", "border-left"}:
                    if is_border_active(val):
                        has_border = True
                elif prop == "box-shadow":
                    if is_shadow_active(val):
                        has_shadow = True
            if has_border and has_shadow:
                issues.append(
                    {
                        "rule": "border-with-shadow",
                        "severity": "warning",
                        "file": rel_path,
                        "line": line_offset + line_number_at(css_clean, block.start()),
                        "snippet": selector[:80],
                        "message": "Surface combines both border and box-shadow; choose one elevation strategy per DESIGN.md.",
                    }
                )

    # 2. Markup & Script checks (.svelte and .html)
    if path.suffix in {".svelte", ".html"}:
        markup = strip_non_markup(text)

        # Emoji used as UI icons
        for m in EMOJI_PATTERN.finditer(markup):
            line_no = line_number_at(markup, m.start())
            lines_list = markup.splitlines()
            line_text = lines_list[line_no - 1].strip() if line_no <= len(lines_list) else m.group(0)
            issues.append(
                {
                    "rule": "emoji-as-ui",
                    "severity": "error",
                    "file": rel_path,
                    "line": line_no,
                    "snippet": line_text[:100],
                    "message": "Replace Unicode emoji in UI markup with a vector Lucide icon.",
                }
            )

        # Heading hierarchy skips within the file
        prev_level: int | None = None
        for m in HEADING_TAG_RE.finditer(markup):
            level = int(m.group(1))
            if prev_level is not None and (level - prev_level) > 1:
                issues.append(
                    {
                        "rule": "heading-skip",
                        "severity": "warning",
                        "file": rel_path,
                        "line": line_number_at(markup, m.start()),
                        "snippet": f"<h{prev_level}> -> <h{level}>",
                        "message": f"Heading level skipped from <h{prev_level}> to <h{level}>; maintain sequential hierarchy.",
                    }
                )
            prev_level = level

        # Accidental Tailwind utility classes
        for m in TAILWIND_CLASS_SMELL_RE.finditer(markup):
            issues.append(
                {
                    "rule": "tailwind-smell",
                    "severity": "error",
                    "file": rel_path,
                    "line": line_number_at(markup, m.start()),
                    "snippet": m.group(0)[:100],
                    "message": "Suspected Tailwind utility classes detected; Lokus Judicial uses native CSS Variables and semantic classes.",
                }
            )

        # Console secret leak check in <script> blocks
        for script_match in SCRIPT_TAG_RE.finditer(text):
            script_body = script_match.group(1)
            offset = text.count("\n", 0, script_match.start(1))
            for m in SECRET_LOG_RE.finditer(script_body):
                issues.append(
                    {
                        "rule": "secret-log",
                        "severity": "error",
                        "file": rel_path,
                        "line": offset + line_number_at(script_body, m.start()),
                        "snippet": m.group(0)[:100],
                        "message": "Potential credential/API key logged to console; remove secret logging immediately.",
                    }
                )

    return issues


def collect_files(root: Path) -> list[Path]:
    files: list[Path] = []
    if root.is_file():
        return [root] if root.suffix in TARGET_EXTENSIONS else []
    for item in sorted(root.rglob("*")):
        if any(part in IGNORED_DIRS for part in item.parts):
            continue
        if item.is_file() and item.suffix in TARGET_EXTENSIONS:
            files.append(item)
    return files


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="audit_ui.py",
        description=(
            "Audit Svelte 5, CSS, and HTML files in Lokus Judicial for UI anti-patterns "
            "(emojis as UI, hardcoded colors, arbitrary radii, border+shadow clashes, "
            "heading skips, Tailwind classes, and console secret leaks).\n\n"
            "Exit codes:\n"
            "  0  No issues found\n"
            "  1  One or more audit issues found\n"
            "  2  Invalid arguments or target path"
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "Examples:\n"
            "  python3 scripts/audit_ui.py --root src --format json\n"
            "  python3 scripts/audit_ui.py --root src/lib/components --limit 25 --offset 0\n"
            "  python3 scripts/audit_ui.py --root src --output audit-report.json"
        ),
    )
    parser.add_argument(
        "--root",
        required=True,
        help="Path to the frontend source directory or single file to audit (e.g., src).",
    )
    parser.add_argument(
        "--format",
        choices=("json", "summary"),
        default="json",
        help="Output format for stdout (default: json).",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=50,
        help="Maximum number of issue objects to include in JSON output (default: 50).",
    )
    parser.add_argument(
        "--offset",
        type=int,
        default=0,
        help="Zero-based pagination offset for issues list (default: 0).",
    )
    parser.add_argument(
        "--output",
        help="Optional file path to write JSON output instead of stdout (use '-' for stdout).",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    root = Path(args.root).resolve()
    if not root.exists():
        print(
            f"Error: --root path does not exist: '{args.root}'. "
            "Pass a valid frontend folder such as --root src.",
            file=sys.stderr,
        )
        return 2

    if args.limit < 1 or args.offset < 0:
        print(
            "Error: --limit must be >= 1 and --offset must be >= 0.",
            file=sys.stderr,
        )
        return 2

    files = collect_files(root)
    print(f"[audit_ui] Scanning {len(files)} file(s) under {root}...", file=sys.stderr)

    all_issues: list[dict] = []
    for file_path in files:
        all_issues.extend(audit_file(file_path, root if root.is_dir() else root.parent))

    by_rule = Counter(issue["rule"] for issue in all_issues)
    by_severity = Counter(issue["severity"] for issue in all_issues)
    sliced = all_issues[args.offset : args.offset + args.limit]

    report = {
        "root": str(root),
        "files_scanned": len(files),
        "total_issues": len(all_issues),
        "returned_issues": len(sliced),
        "offset": args.offset,
        "limit": args.limit,
        "has_more": (args.offset + len(sliced)) < len(all_issues),
        "summary_by_rule": dict(sorted(by_rule.items())),
        "summary_by_severity": dict(sorted(by_severity.items())),
        "issues": sliced,
    }

    if args.format == "summary":
        rendered = (
            f"files_scanned={report['files_scanned']} "
            f"total_issues={report['total_issues']} "
            f"by_severity={json.dumps(report['summary_by_severity'])} "
            f"by_rule={json.dumps(report['summary_by_rule'])}"
        )
    else:
        rendered = json.dumps(report, indent=2, ensure_ascii=False)

    if args.output and args.output != "-":
        out_path = Path(args.output)
        out_path.write_text(rendered + "\n", encoding="utf-8")
        print(f"[audit_ui] Report written to {out_path}", file=sys.stderr)
    else:
        print(rendered)

    return 1 if all_issues else 0


if __name__ == "__main__":
    sys.exit(main())
