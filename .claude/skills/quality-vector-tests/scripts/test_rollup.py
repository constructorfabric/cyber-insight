from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path

from rollup import VECTORS, rollup

SCENARIO = "- [ ] 1. **Synthetic refusal** — Security · manual · AC-1 — submit → reject."
NFR = """## 6. Non-Functional Requirements
### 6.1 NFR Inclusions
#### Synthetic boundary
- [ ] `p1` - **ID**: `cpt-example-nfr-boundary`
**Vector**: Security"""


class RollupTests(unittest.TestCase):
    def test_scenarios_count_only_in_testing(self) -> None:
        for heading in ("## Testing", "## 7. Testing"):
            for checkbox in ("[ ]", "[x]", "[X]"):
                with self.subTest(heading=heading, checkbox=checkbox):
                    text = f"## Background\n{SCENARIO}\n{heading}\n{SCENARIO.replace('[ ]', checkbox)}\n## Notes\n{SCENARIO}"
                    self.assertEqual(rollup(text)["Security"], "1")

    def test_examples_and_comments_do_not_claim_vectors(self) -> None:
        examples = (
            f"```markdown\n{SCENARIO}\n```",
            f"~~~~markdown\n{SCENARIO}\n~~~\n{SCENARIO}\n~~~~",
            f"<!--\n{SCENARIO}\n-->",
            f"> {SCENARIO}",
            f"    {SCENARIO}",
            "**Security** — n/a: <!-- no reason -->",
        )
        for example in examples:
            with self.subTest(example=example):
                self.assertEqual(rollup(f"## Testing\n{example}")["Security"], "MISSING")

    def test_fenced_comment_markup_does_not_hide_later_scenarios(self) -> None:
        text = f"## Testing\n```html\n<!--\n```\n{SCENARIO}"
        self.assertEqual(rollup(text)["Security"], "1")

    def test_exclusion_reasons_do_not_declare_other_vectors(self) -> None:
        text = (
            "## 6. Non-Functional Requirements\n### 6.2 NFR Exclusions\n- **Performance**: measured alongside Security."
        )
        result = rollup(text)
        self.assertEqual(result["Performance"], "n/a (declared)")
        self.assertEqual(result["Security"], "MISSING")

    def test_non_applicability_needs_a_scoped_declaration_and_reason(self) -> None:
        cases = (
            ("## Testing\n**Security** — n/a: no authorization change.", "n/a (declared)"),
            ("## Background\n**Security** — n/a: no authorization change.", "MISSING"),
            ("## Testing\n**Security** — n/a:", "MISSING"),
            ("## Testing\n**Security** is not n/a", "MISSING"),
            ("## 6. Non-Functional Requirements\n### 6.2 NFR Exclusions\n- Security:", "MISSING"),
        )
        for text, expected in cases:
            with self.subTest(text=text):
                self.assertEqual(rollup(text)["Security"], expected)

    def test_inherited_obligations_remain_distinct_from_local_nfrs(self) -> None:
        inherited = "\n#### Shared authorization\n**Vector**: Security\n**Inherits**: `cpt-shared-nfr-authorization`\n**Verification**: synthetic shared suite; owner: Example team."
        self.assertEqual(rollup(NFR)["Security"], "1")
        self.assertEqual(rollup(NFR + inherited)["Security"], "1 + inherited (1)")
        self.assertEqual(
            rollup("## 6. Non-Functional Requirements\n### 6.1 NFR Inclusions" + inherited)["Security"], "inherited (1)"
        )

    def test_nfr_examples_and_unrelated_vector_fields_do_not_count(self) -> None:
        text = "## Background\n**Vector**: Reliability\n```markdown\n" + NFR + "\n```"
        self.assertEqual(rollup(text), dict.fromkeys(VECTORS, "MISSING"))

    def test_invalid_nfr_attribution_and_conflicting_exclusion_fail(self) -> None:
        cases = (
            NFR + "\n**Vector**: Reliability",
            NFR.replace("**Vector**: Security", "**Vector**: Security, Reliability"),
            NFR + "\n**Inherits**: `cpt-shared-nfr-authorization`",
            NFR + "\n### 6.2 NFR Exclusions\n- Security: no security work.",
        )
        for text in cases:
            with self.subTest(text=text), self.assertRaises(ValueError):
                rollup(text)

    def test_report_order_does_not_depend_on_scenario_order(self) -> None:
        scenarios = [SCENARIO.replace("Security", vector) for vector in reversed(VECTORS)]
        self.assertEqual(list(rollup("## Testing\n" + "\n".join(scenarios))), list(VECTORS))

    def test_cli_reports_read_and_parse_errors_without_partial_counts(self) -> None:
        script = Path(__file__).with_name("rollup.sh")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            invalid = root / "invalid.md"
            invalid.write_text(NFR + "\n**Vector**: Reliability", encoding="utf-8")
            for path in (root / "absent.md", root, invalid):
                with self.subTest(path=path):
                    result = subprocess.run(
                        ["bash", str(script), str(path)], capture_output=True, text=True, check=False
                    )
                    self.assertEqual(result.returncode, 2)
                    self.assertEqual(result.stdout, "")
                    self.assertIn("error:", result.stderr)


if __name__ == "__main__":
    unittest.main()
