#!/usr/bin/env python3
"""Run the deploy script with fake tools; never contact GCP or build images."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent
MOCK_TOOL = """#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
tool = Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ['DEPLOY_TEST_LOG'], 'a') as log:
    log.write(json.dumps([tool, *args]) + '\\n')
if tool == 'git' and args[:1] == ['rev-parse']:
    print('abc123456789')
if tool == 'gcloud' and args[:3] == ['config', 'get-value', 'project']:
    print('deploy-test')
if tool == 'gcloud' and args[:3] == ['run', 'services', 'describe']:
    print('https://api.example.run.app')
"""


class RedeployTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "scripts").mkdir()
        shutil.copy2(ROOT / "scripts/redeploy-gcp.sh", self.root / "scripts")
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for tool in ("docker", "gcloud", "git"):
            path = self.bin / tool
            path.write_text(MOCK_TOOL)
            path.chmod(0o755)
        self.log = self.root / "calls.jsonl"
        self.env = {
            "PATH": f"{self.bin}:{os.defpath}",
            "DEPLOY_TEST_LOG": str(self.log),
            "SKIP_CHECKS": "1",
            "VITE_MERCADOPAGO_PUBLIC_KEY": "test-public-key",
        }

    def deploy(self, *args, extra=""):
        self.env["API_DEPLOY_EXTRA_ARGS"] = extra
        return subprocess.run(
            ["bash", str(self.root / "scripts/redeploy-gcp.sh"), *args],
            env=self.env,
            capture_output=True,
            text=True,
            timeout=10,
        )

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_api_preserves_existing_credentials(self):
        result = self.deploy("api", extra="--update-env-vars FEATURE_ENABLED=true")
        self.assertEqual(result.returncode, 0, result.stderr)
        deploy = next(c for c in self.calls() if c[:3] == ["gcloud", "run", "deploy"])
        self.assertIn("APP_ENV=production", deploy)
        self.assertIn("FEATURE_ENABLED=true", deploy)
        self.assertFalse(any("GOOGLE_" in arg or "APP_ENCRYPTION_KEY" in arg for arg in deploy))
        self.assertTrue(any(c[:4] == ["gcloud", "run", "services", "update-traffic"] for c in self.calls()))

    def test_destructive_flags_abort_before_any_tools(self):
        for flag in (
            "--set-env-vars", "--env-vars-file", "--clear-env-vars", "--remove-env-vars",
            "--set-secrets", "--clear-secrets", "--remove-secrets", "--flags-file",
        ):
            for extra in (f"{flag}=example", f"--update-env-vars FEATURE=true {flag} example"):
                with self.subTest(extra=extra):
                    result = self.deploy("all", extra=extra)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("cannot replace or remove", result.stderr)
                    self.assertFalse(self.log.exists())

    def test_candidate_does_not_promote_traffic(self):
        result = self.deploy("--no-traffic", "api", extra="--update-secrets CRON_SECRET=cron-secret:2")
        self.assertEqual(result.returncode, 0, result.stderr)
        deploy = next(c for c in self.calls() if c[:3] == ["gcloud", "run", "deploy"])
        self.assertIn("--no-traffic", deploy)
        self.assertIn("--tag=candidate", deploy)
        self.assertIn("CRON_SECRET=cron-secret:2", deploy)
        self.assertFalse(any("update-traffic" in c for c in self.calls()))


if __name__ == "__main__":
    unittest.main()
