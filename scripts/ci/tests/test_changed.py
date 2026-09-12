from __future__ import annotations

import fnmatch
import json
import re
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "scripts" / "ci" / "changed.py"
BUILD_IMAGES = ROOT / ".github" / "workflows" / "build-images.yml"
HEALTH_SCRIPT = ROOT / "src" / "backend" / "services" / "insight-v3-core" / "tests" / "health.sh"
CI_DIR = ROOT / "scripts" / "ci"
sys.path.insert(0, str(CI_DIR))

import changed  # noqa: E402
from components import COMPONENTS  # noqa: E402


class ChangedCliTests(unittest.TestCase):
    def test_insight_v3_core_health_allows_a_cold_ci_build(self) -> None:
        self.assertIn("for _ in {1..240}; do", HEALTH_SCRIPT.read_text())

    def test_insight_v3_core_image_is_in_the_delivery_workflow(self) -> None:
        workflow = BUILD_IMAGES.read_text()

        for required in [
            "insight_v3_core: ${{ steps.filter.outputs.insight_v3_core }}",
            "src/backend/services/insight-v3-core/**",
            "backend-insight-v3-core:",
            "merge-insight-v3-core:",
            "${{ env.IMAGE_PREFIX }}/insight-v3-core",
            "INSIGHT_V3_CORE: ${{ needs.changes.outputs.insight_v3_core }}",
            "src/backend/services/insight-v3-core/helm/Chart.yaml",
            "|| needs.changes.outputs.insight_v3_core == 'true'",
        ]:
            self.assertIn(required, workflow)
        self.assertGreaterEqual(workflow.count("- merge-insight-v3-core"), 2)

    def test_compare_ref_selects_the_diff_base(self) -> None:
        result = subprocess.run(
            ["python3", str(SCRIPT), "--compare-ref", "HEAD"], cwd=ROOT, capture_output=True, text=True, check=False
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), {"rust": [], "python": [], "js": []})

    def test_insight_v3_core_change_schedules_its_rust_job(self) -> None:
        completed = subprocess.CompletedProcess(
            args=["git", "diff"], returncode=0, stdout="src/backend/services/insight-v3-core/src/gear.rs\n"
        )

        with patch.object(changed.subprocess, "run", return_value=completed):
            matrix = changed.changed_components("origin/main", COMPONENTS)

        jobs = [job for job in matrix["rust"] if job["name"] == "insight-v3-core"]
        self.assertEqual(len(jobs), 1)
        self.assertEqual(
            jobs[0],
            {
                "name": "insight-v3-core",
                "root": "src/backend",
                "package": "insight-v3-core",
                "all_features": True,
                "lint": True,
                "cover": False,
                "test": True,
                "clippy": True,
                "live_db": True,
                "live_ch": True,
                "live_test": "services/insight-v3-core/tests/ci.sh",
                "live_db_name": "insight_v3",
                "cover_ignore_regex": "",
            },
        )

    def test_insight_clickhouse_change_runs_insight_v3_core_tests(self) -> None:
        completed = subprocess.CompletedProcess(
            args=["git", "diff"], returncode=0, stdout="src/backend/libs/insight-clickhouse/src/lib.rs\n"
        )

        with patch.object(changed.subprocess, "run", return_value=completed):
            matrix = changed.changed_components("origin/main", COMPONENTS)

        core_job = next(job for job in matrix["rust"] if job["name"] == "insight-v3-core")
        self.assertTrue(core_job["test"])

    def test_seed_only_diff_schedules_the_seed_component(self) -> None:
        completed = subprocess.CompletedProcess(
            args=["git", "diff"], returncode=0, stdout="src/ingestion/tools/seed/insight_seed/manifest.py\n"
        )

        with patch.object(changed.subprocess, "run", return_value=completed):
            matrix = changed.changed_components("origin/main", COMPONENTS)

        self.assertEqual(matrix["rust"], [])
        self.assertEqual(matrix["js"], [])
        self.assertEqual([job["name"] for job in matrix["python"]], ["insight-seed"])
        job = matrix["python"][0]
        self.assertEqual(job["root"], "src/ingestion/tools/seed")
        self.assertFalse(job["cover"])
        self.assertFalse(job["collect"])

    def test_ci_trigger_paths_cover_every_component_path(self) -> None:
        """A path owned by a registered component must start ci.yml, or a PR
        touching only that component never schedules its producer job."""
        workflow = (ROOT / ".github" / "workflows" / "ci.yml").read_text()
        globs = re.findall(r'- "([^"]+)"', workflow)

        for comp in COMPONENTS:
            for owned in comp["paths"]:
                probe = owned if "." in owned.rsplit("/", 1)[-1] else owned + "/file"
                self.assertTrue(
                    any(fnmatch.fnmatch(probe, g) for g in globs), f"ci.yml on.paths misses component path: {owned}"
                )
        for glob in set(globs):
            self.assertEqual(
                workflow.count(f'"{glob}"'), 2, f"trigger path not mirrored across pull_request and push: {glob}"
            )

    def test_lockfile_change_runs_every_openapi_drift_test(self) -> None:
        completed = subprocess.CompletedProcess(args=["git", "diff"], returncode=0, stdout="src/backend/Cargo.lock\n")

        with patch.object(changed.subprocess, "run", return_value=completed):
            matrix = changed.changed_components("origin/main", COMPONENTS)

        tested = sorted(job["name"] for job in matrix["rust"] if job["test"])
        drift_gated = sorted(c["name"] for c in COMPONENTS if c.get("drift_test"))
        self.assertEqual(tested, drift_gated)
        self.assertFalse(any(job["cover"] for job in matrix["rust"]))


if __name__ == "__main__":
    unittest.main()
