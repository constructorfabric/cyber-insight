from __future__ import annotations

import json
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "scripts" / "ci" / "changed.py"
CI_DIR = ROOT / "scripts" / "ci"
sys.path.insert(0, str(CI_DIR))

import changed  # noqa: E402
from components import COMPONENTS  # noqa: E402


class ChangedCliTests(unittest.TestCase):
    def test_compare_ref_selects_the_diff_base(self) -> None:
        result = subprocess.run(
            ["python3", str(SCRIPT), "--compare-ref", "HEAD"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), {"rust": [], "python": [], "js": []})

    def test_insight_v3_core_change_schedules_its_rust_job(self) -> None:
        completed = subprocess.CompletedProcess(
            args=["git", "diff"],
            returncode=0,
            stdout="src/backend/services/insight-v3-core/src/gear.rs\\n",
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
                "live_db": False,
                "live_ch": False,
                "live_db_name": "insight-v3-core",
                "cover_ignore_regex": "",
            },
        )


if __name__ == "__main__":
    unittest.main()
