import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from check_docs import (  # noqa: E402
    MAX_AGENTS_BYTES,
    ROOT as REPO_ROOT,
    check_adr_files_and_index,
    check_agents_doc,
    check_all,
    check_status_table,
)


def _valid_adr_0048() -> str:
    return (
        "# ADR 0048：文档治理\n\n"
        "- 日期：2026-11-14\n"
        "- 状态：Accepted\n"
        "- 关联：P2-8\n\n"
        "## 背景\n\n"
        "参考 [评审](../reviews/2026-11-14-architecture-review.md)。\n\n"
        "## 选项\n\n1. 人工约定。\n2. 脚本校验。\n\n"
        "## 决定\n\n采用脚本校验。\n\n"
        "## 后果\n\n减少漂移。\n"
    )


class DocsGovernanceTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)

    def write(self, rel: str, text: str) -> Path:
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def _seed_minimal_valid_repo(self) -> None:
        self.write("AGENTS.md", "# 约定\n\n仅保留不变量。\n")
        self.write(
            "docs/architecture/reviews/2026-11-14-architecture-review.md",
            "# Review\n",
        )
        self.write("docs/architecture/adr/0048-doc-governance.md", _valid_adr_0048())
        self.write(
            "docs/architecture/README.md",
            "# 架构\n\n<!-- ADR-INDEX-START -->\n"
            "| 编号 | 标题 | 状态 | 一句话决策 |\n"
            "|---|---|---|---|\n"
            "| 0048 | [文档治理](adr/0048-doc-governance.md) | Accepted | 单一状态表与 ADR 校验 |\n"
            "<!-- ADR-INDEX-END -->\n",
        )
        self.write("experiments/demo/RESULTS.md", "# Demo\n")
        self.write(
            "docs/status.md",
            "# 状态表\n\n<!-- STATUS-TABLE-START -->\n"
            "| Feature / 模块 | 源码 | 构建 | 本机运行 | 模拟器 | 真机 | 证据与关联 ADR |\n"
            "|---|---|---|---|---|---|---|\n"
            "| P2-8 文档治理 | 已落位 | 通过 | 无界面通过 | 不适用 | 不适用 | [Demo](../experiments/demo/RESULTS.md) · [ADR 0048](architecture/adr/0048-doc-governance.md) |\n"
            "<!-- STATUS-TABLE-END -->\n",
        )

    def test_accepts_valid_minimal_repo(self):
        self._seed_minimal_valid_repo()
        errors, _ = check_all(self.root)
        self.assertEqual(errors, [])

    def test_rejects_oversized_agents_and_progress_section(self):
        self._seed_minimal_valid_repo()
        self.write("AGENTS.md", "中" * ((MAX_AGENTS_BYTES // 3) + 10))
        self.assertTrue(check_agents_doc(self.root))

        self.write("AGENTS.md", "# 约定\n\n## 当前阶段\n\n流水。\n")
        self.assertTrue(check_agents_doc(self.root))

    def test_rejects_adr_missing_fields_sections_or_bad_status(self):
        self._seed_minimal_valid_repo()
        self.write(
            "docs/architecture/adr/0048-doc-governance.md",
            "# ADR 0048：坏状态\n\n"
            "- 日期：2026-11-14\n"
            "- 状态：实施中\n"
            "- 关联：P2-8\n\n"
            "## 背景\n\n## 决定\n\n## 后果\n",
        )
        errors = check_adr_files_and_index(self.root)
        self.assertTrue(any("Proposed|Accepted|Superseded" in e for e in errors))
        self.assertTrue(any("## 选项" in e for e in errors))

    def test_rejects_adr_progress_date_narration_and_test_counts(self):
        self._seed_minimal_valid_repo()
        for bad_line in (
            "2026-11-15：新增实现并合入主干。",
            "- 2026-11-15 完成本地构建与端到端回归。",
            "本轮在 macOS 上通过 35 项测试。",
        ):
            with self.subTest(bad_line=bad_line):
                self.write(
                    "docs/architecture/adr/0048-doc-governance.md",
                    _valid_adr_0048() + f"\n{bad_line}\n",
                )
                errors = check_adr_files_and_index(self.root)
                self.assertTrue(
                    any("forbidden" in e for e in errors),
                    f"expected progress rejection for: {bad_line}, got: {errors}",
                )

    def test_rejects_adr_index_mismatch(self):
        self._seed_minimal_valid_repo()
        self.write(
            "docs/architecture/adr/0049-next.md",
            _valid_adr_0048().replace("ADR 0048", "ADR 0049"),
        )
        errors = check_adr_files_and_index(self.root)
        self.assertTrue(any("missing ADR 0049" in e for e in errors))

    def test_rejects_status_table_missing_evidence_or_bad_state(self):
        self._seed_minimal_valid_repo()
        self.write(
            "docs/status.md",
            "# 状态表\n\n<!-- STATUS-TABLE-START -->\n"
            "| Feature / 模块 | 源码 | 构建 | 本机运行 | 模拟器 | 真机 | 证据与关联 ADR |\n"
            "|---|---|---|---|---|---|---|\n"
            "| F-DEMO | 已完成 | 通过 | 实窗通过 | 未验证 | 未验证 | 暂无链接 |\n"
            "<!-- STATUS-TABLE-END -->\n",
        )
        errors = check_status_table(self.root)
        self.assertTrue(any("invalid 源码 state" in e for e in errors))
        self.assertTrue(any("without evidence link" in e for e in errors))

        self.write(
            "docs/status.md",
            "# 状态表\n\n<!-- STATUS-TABLE-START -->\n"
            "| Feature / 模块 | 源码 | 构建 | 本机运行 | 模拟器 | 真机 | 证据与关联 ADR |\n"
            "|---|---|---|---|---|---|---|\n"
            "| F-DEMO | 已落位 | 通过 | 实窗通过 | 未验证 | 未验证 | [不存在](../experiments/missing.md) |\n"
            "<!-- STATUS-TABLE-END -->\n",
        )
        errors = check_status_table(self.root)
        self.assertTrue(any("missing evidence file" in e for e in errors))

    def test_current_repository_passes_docs_governance(self):
        errors, count = check_all(REPO_ROOT)
        self.assertGreater(count, 100)
        self.assertEqual(errors, [])


if __name__ == "__main__":
    unittest.main()
