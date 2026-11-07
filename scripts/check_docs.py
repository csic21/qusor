"""Check local Markdown file references; remote URLs and heading anchors are excluded."""
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
SKIP = {".git", "target", "__pycache__", "node_modules"}
LINK = re.compile(r"(?<!!)\[[^\]]+\]\((<[^>]+>|[^)\n]+)\)")


def main() -> int:
    errors = []
    count = 0
    for path in sorted(ROOT.rglob("*.md")):
        if any(part in SKIP for part in path.relative_to(ROOT).parts):
            continue
        count += 1
        source = path.read_text(encoding="utf-8-sig")
        source = re.sub(r"(?ms)^\x60\x60\x60.*?^\x60\x60\x60[ \t]*$", "", source)
        for match in LINK.finditer(source):
            target = match.group(1).strip().strip("<>")
            if urlsplit(target).scheme or target.startswith("//"):
                continue
            local_path = unquote(urlsplit(target).path)
            if not local_path:
                continue
            resolved = (path.parent / local_path).resolve()
            if not resolved.is_relative_to(ROOT) or not resolved.exists():
                errors.append(f"{path.relative_to(ROOT)}: missing/local-outside link {target}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"Local file links passed in {count} Markdown files (anchors/remote URLs not checked).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
