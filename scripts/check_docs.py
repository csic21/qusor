"""Check Markdown links, AGENTS.md size, ADR metadata/index, and docs/status.md."""
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
SKIP = {".git", "target", "__pycache__", "node_modules", ".local", ".nsh"}
LINK = re.compile(r"(?<!!)\[[^\]]+\]\((<[^>]+>|[^)\n]+)\)")
FENCED_CODE = re.compile(r"(?ms)^\x60\x60\x60.*?^\x60\x60\x60[ \t]*$")
INLINE_CODE = re.compile(r"`[^`\n]+`")

MAX_AGENTS_BYTES = 4350
STRICT_ADR_MIN_NUMBER = 48

ADR_FILENAME = re.compile(r"^(\d{4})-[a-z0-9-]+\.md$")
ADR_INDEX_BLOCK = re.compile(
    r"<!--\s*ADR-INDEX-START\s*-->(.*?)<!--\s*ADR-INDEX-END\s*-->", re.DOTALL
)
STATUS_TABLE_BLOCK = re.compile(
    r"<!--\s*STATUS-TABLE-START\s*-->(.*?)<!--\s*STATUS-TABLE-END\s*-->", re.DOTALL
)

VALID_ADR_STATUSES = {"Proposed", "Accepted", "Superseded"}
ALLOWED_SOURCE_STATES = {"未开始", "设计中", "进行中", "实验原型", "已落位"}
ALLOWED_BUILD_STATES = {"未验证", "通过", "部分通过", "失败", "受限"}
ALLOWED_LOCAL_RUN_STATES = {
    "未验证",
    "无界面通过",
    "实窗通过",
    "实验通过",
    "部分通过",
    "不适用",
}
ALLOWED_SIM_DEVICE_STATES = {"未验证", "通过", "不适用"}
EVIDENCE_REQUIRED_RUN_STATES = {
    "通过",
    "无界面通过",
    "实窗通过",
    "实验通过",
    "部分通过",
}

DATE_LEADING_PROGRESS = re.compile(r"^\s*(?:[-*]\s*|#+\s*)?20\d{2}-\d{2}-\d{2}\b")
DATE_WITH_PROGRESS_VERB = re.compile(
    r"20\d{2}-\d{2}-\d{2}[^\n]*(?:已完成|已通过|通过了|已落地|已实现|已验证|推进到|合入|增量|回归通过|自测通过)"
)
COUNT_PROGRESS_LOG = re.compile(
    r"(?:通过|跑通|完成|已过)\s*\d+\s*项(?:测试|回归|断言|场景|用例)"
)


def _strip_code_and_link_targets(text: str) -> str:
    text = FENCED_CODE.sub("", text)
    text = INLINE_CODE.sub("", text)
    text = re.sub(r"\]\([^)\n]+\)", "]()", text)
    return text


def _parse_markdown_table(block: str) -> list[list[str]]:
    rows = []
    for raw_line in block.splitlines():
        line = raw_line.strip()
        if not line.startswith("|") or not line.endswith("|"):
            continue
        cells = [c.strip() for c in line.strip("|").split("|")]
        if all(re.fullmatch(r":?-{3,}:?", c) for c in cells):
            continue
        rows.append(cells)
    return rows


def check_markdown_links(root: Path) -> tuple[list[str], int]:
    errors: list[str] = []
    count = 0
    for path in sorted(root.rglob("*.md")):
        if any(part in SKIP for part in path.relative_to(root).parts):
            continue
        count += 1
        source = path.read_text(encoding="utf-8-sig")
        source = FENCED_CODE.sub("", source)
        for match in LINK.finditer(source):
            target = match.group(1).strip().strip("<>")
            if urlsplit(target).scheme or target.startswith("//"):
                continue
            local_path = unquote(urlsplit(target).path)
            if not local_path:
                continue
            resolved = (path.parent / local_path).resolve()
            if not resolved.is_relative_to(root.resolve()) or not resolved.exists():
                errors.append(
                    f"{path.relative_to(root)}: missing/local-outside link {target}"
                )
    return errors, count


def check_agents_doc(root: Path, max_bytes: int = MAX_AGENTS_BYTES) -> list[str]:
    agents_path = root / "AGENTS.md"
    if not agents_path.exists():
        return ["AGENTS.md: file is missing"]
    raw = agents_path.read_bytes()
    errors: list[str] = []
    if len(raw) > max_bytes:
        errors.append(
            f"AGENTS.md: size {len(raw)} bytes exceeds cap {max_bytes} bytes"
        )
    text = raw.decode("utf-8-sig")
    if re.search(r"^##\s+当前阶段\b", text, re.MULTILINE):
        errors.append("AGENTS.md: must not contain '## 当前阶段' progress section")
    return errors


def check_adr_files_and_index(root: Path) -> list[str]:
    errors: list[str] = []
    adr_dir = root / "docs" / "architecture" / "adr"
    readme_path = root / "docs" / "architecture" / "README.md"
    if not adr_dir.is_dir():
        return ["docs/architecture/adr: directory is missing"]
    if not readme_path.exists():
        return ["docs/architecture/README.md: file is missing"]

    adr_files = sorted(adr_dir.glob("*.md"))
    by_num: dict[str, Path] = {}
    file_statuses: dict[str, str] = {}
    for path in adr_files:
        rel = path.relative_to(root)
        m = ADR_FILENAME.match(path.name)
        if not m:
            errors.append(f"{rel}: ADR filename must match NNNN-slug.md")
            continue
        num = m.group(1)
        if num in by_num:
            errors.append(f"{rel}: duplicate ADR number {num}")
        else:
            by_num[num] = path
        adr_errors, adr_status = _check_single_adr(path, root, int(num))
        errors.extend(adr_errors)
        if adr_status is not None:
            file_statuses[num] = adr_status

    readme_text = readme_path.read_text(encoding="utf-8-sig")
    block_match = ADR_INDEX_BLOCK.search(readme_text)
    if not block_match:
        errors.append(
            "docs/architecture/README.md: missing <!-- ADR-INDEX-START --> ... <!-- ADR-INDEX-END --> block"
        )
        return errors

    rows = _parse_markdown_table(block_match.group(1))
    if len(rows) < 2:
        errors.append("docs/architecture/README.md: ADR index table has no entries")
        return errors

    header = rows[0]
    if header != ["编号", "标题", "状态", "一句话决策"]:
        errors.append(
            f"docs/architecture/README.md: unexpected ADR index header {header}"
        )

    indexed_nums: dict[str, Path] = {}
    prev_num = -1
    for row in rows[1:]:
        if len(row) != 4:
            errors.append(
                f"docs/architecture/README.md: ADR index row must have 4 columns: {row}"
            )
            continue
        num_str, title_col, status_col, summary_col = row
        if not re.fullmatch(r"\d{4}", num_str):
            errors.append(
                f"docs/architecture/README.md: invalid ADR number '{num_str}'"
            )
            continue
        num_val = int(num_str)
        if num_val <= prev_num:
            errors.append(
                f"docs/architecture/README.md: ADR index not strictly ascending at {num_str}"
            )
        prev_num = num_val

        link_m = LINK.search(title_col)
        if not link_m:
            errors.append(
                f"docs/architecture/README.md: ADR {num_str} missing markdown link"
            )
            continue
        target = link_m.group(1).strip().strip("<>")
        resolved = (readme_path.parent / unquote(urlsplit(target).path)).resolve()
        if not resolved.exists() or resolved.parent != adr_dir.resolve():
            errors.append(
                f"docs/architecture/README.md: ADR {num_str} target invalid: {target}"
            )
            continue
        if not resolved.name.startswith(f"{num_str}-"):
            errors.append(
                f"docs/architecture/README.md: ADR {num_str} points to mismatched file {resolved.name}"
            )
        if num_str in indexed_nums:
            errors.append(
                f"docs/architecture/README.md: duplicate ADR {num_str} in index"
            )
        indexed_nums[num_str] = resolved

        if status_col not in VALID_ADR_STATUSES:
            errors.append(
                f"docs/architecture/README.md: ADR {num_str} invalid status '{status_col}'"
            )
        elif num_val >= STRICT_ADR_MIN_NUMBER and num_str in file_statuses:
            if status_col != file_statuses[num_str]:
                errors.append(
                    f"docs/architecture/README.md: ADR {num_str} index status '{status_col}' "
                    f"mismatches ADR file status '{file_statuses[num_str]}'"
                )
        if not summary_col:
            errors.append(
                f"docs/architecture/README.md: ADR {num_str} summary cannot be empty"
            )

    missing_in_index = sorted(set(by_num) - set(indexed_nums))
    for num in missing_in_index:
        errors.append(
            f"docs/architecture/README.md: missing ADR {num} ({by_num[num].name}) in index table"
        )
    return errors


def _check_single_adr(path: Path, root: Path, num: int) -> tuple[list[str], str | None]:
    errors: list[str] = []
    rel = path.relative_to(root)
    text = path.read_text(encoding="utf-8-sig")
    lines = text.splitlines()
    if not lines or not re.match(rf"^#\s+ADR\s+{num:04d}\b", lines[0].strip()):
        errors.append(f"{rel}: H1 must start with '# ADR {num:04d}'")

    header_text = "\n".join(lines[:15])
    if num < STRICT_ADR_MIN_NUMBER:
        if "日期" not in header_text or "状态" not in header_text:
            errors.append(f"{rel}: historical ADR missing 日期/状态 header")
        return errors, None

    # Strict ADR 0048+ governance checks
    fields: dict[str, str] = {}
    header_end_idx = len(lines)
    for idx, line in enumerate(lines[:20]):
        if idx > 0 and line.startswith("## "):
            header_end_idx = idx
            break
        m = re.match(r"^\s*(?:[-*]\s*)?(日期|状态|关联)[：:]\s*(.+)$", line)
        if m:
            fields[m.group(1)] = m.group(2).strip()

    for required in ("日期", "状态", "关联"):
        if required not in fields or not fields[required]:
            errors.append(f"{rel}: missing required header field '- {required}：...'")

    if "日期" in fields and not re.fullmatch(r"\d{4}-\d{2}-\d{2}", fields["日期"]):
        errors.append(f"{rel}: '日期' must be YYYY-MM-DD, got '{fields['日期']}'")

    primary_status: str | None = None
    if "状态" in fields:
        status_m = re.match(
            r"^(Proposed|Accepted|Superseded)(?:$|\s|[（(])", fields["状态"]
        )
        if not status_m:
            errors.append(
                f"{rel}: '状态' must start with Proposed|Accepted|Superseded, got '{fields['状态']}'"
            )
        else:
            primary_status = status_m.group(1)

    h2_headings = {
        line.strip().lstrip("# ").strip()
        for line in lines
        if line.startswith("## ")
    }
    for req_heading in ("背景", "选项", "后果"):
        if req_heading not in h2_headings:
            errors.append(f"{rel}: missing required section '## {req_heading}'")
    if "决定" not in h2_headings and "决策" not in h2_headings:
        errors.append(f"{rel}: missing required section '## 决定' (or '## 决策')")

    body = "\n".join(lines[header_end_idx:])
    clean_body = _strip_code_and_link_targets(body)
    for line in clean_body.splitlines():
        if DATE_LEADING_PROGRESS.search(line):
            errors.append(
                f"{rel}: forbidden date-led progress log line in ADR body: {line.strip()}"
            )
        elif DATE_WITH_PROGRESS_VERB.search(line):
            errors.append(
                f"{rel}: forbidden dated progress narration in ADR body: {line.strip()}"
            )
        elif COUNT_PROGRESS_LOG.search(line):
            errors.append(
                f"{rel}: forbidden test/count progress log in ADR body: {line.strip()}"
            )

    return errors, primary_status


def check_status_table(root: Path) -> list[str]:
    status_path = root / "docs" / "status.md"
    if not status_path.exists():
        return ["docs/status.md: file is missing"]
    text = status_path.read_text(encoding="utf-8-sig")
    block_match = STATUS_TABLE_BLOCK.search(text)
    if not block_match:
        return [
            "docs/status.md: missing <!-- STATUS-TABLE-START --> ... <!-- STATUS-TABLE-END --> block"
        ]

    rows = _parse_markdown_table(block_match.group(1))
    if len(rows) < 2:
        return ["docs/status.md: status table has no entries"]

    errors: list[str] = []
    expected_header = [
        "Feature / 模块",
        "源码",
        "构建",
        "本机运行",
        "模拟器",
        "真机",
        "证据与关联 ADR",
    ]
    if rows[0] != expected_header:
        errors.append(f"docs/status.md: unexpected header {rows[0]}")
        return errors

    for idx, row in enumerate(rows[1:], start=1):
        if len(row) != 7:
            errors.append(f"docs/status.md: row {idx} must have 7 columns: {row}")
            continue
        feature, src, build, local_run, sim, device, evidence = row
        if not feature:
            errors.append(f"docs/status.md: row {idx} Feature / 模块 is empty")
        if src not in ALLOWED_SOURCE_STATES:
            errors.append(f"docs/status.md: row {idx} invalid 源码 state '{src}'")
        if build not in ALLOWED_BUILD_STATES:
            errors.append(f"docs/status.md: row {idx} invalid 构建 state '{build}'")
        if local_run not in ALLOWED_LOCAL_RUN_STATES:
            errors.append(
                f"docs/status.md: row {idx} invalid 本机运行 state '{local_run}'"
            )
        if sim not in ALLOWED_SIM_DEVICE_STATES:
            errors.append(f"docs/status.md: row {idx} invalid 模拟器 state '{sim}'")
        if device not in ALLOWED_SIM_DEVICE_STATES:
            errors.append(f"docs/status.md: row {idx} invalid 真机 state '{device}'")

        has_run_claim = (
            local_run in EVIDENCE_REQUIRED_RUN_STATES
            or sim in EVIDENCE_REQUIRED_RUN_STATES
            or device in EVIDENCE_REQUIRED_RUN_STATES
        )
        links = LINK.findall(evidence)
        if has_run_claim and not links:
            errors.append(
                f"docs/status.md: row {idx} ({feature}) claims run/sim/device pass without evidence link"
            )
        for raw_target in links:
            target = raw_target.strip().strip("<>")
            if urlsplit(target).scheme or target.startswith("//"):
                continue
            resolved = (status_path.parent / unquote(urlsplit(target).path)).resolve()
            if not resolved.is_relative_to(root.resolve()) or not resolved.exists():
                errors.append(
                    f"docs/status.md: row {idx} missing evidence file {target}"
                )
    return errors


def check_all(root: Path) -> tuple[list[str], int]:
    errors, md_count = check_markdown_links(root)
    errors.extend(check_agents_doc(root))
    errors.extend(check_adr_files_and_index(root))
    errors.extend(check_status_table(root))
    return errors, md_count


def main() -> int:
    errors, count = check_all(ROOT)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(
        f"Docs governance & local file links passed in {count} Markdown files "
        "(anchors/remote URLs not checked)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
