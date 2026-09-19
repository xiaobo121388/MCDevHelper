import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile


class ExportExampleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.source = Path(self.temp.name) / "source \u4e2d\u6587"
        self.output = Path(self.temp.name) / "output"
        self.source.mkdir()
        self.output.mkdir()
        (self.source / ".empty").mkdir()
        (self.source / ".hidden").write_text("hidden", encoding="utf-8")
        (self.source / "main.pyi").write_text("stub", encoding="utf-8")
        self.script = Path(__file__).with_name("pack.py")

    def run_pack(self, *arguments, environment=None):
        env = dict(os.environ, PYTHONUTF8="1")
        env.update(environment or {})
        return subprocess.run([sys.executable, "-u", str(self.script), *arguments],
                              env=env, capture_output=True, encoding="utf-8", timeout=15)

    def test_arguments_and_environment_preserve_content(self):
        name = "release & % \u4e2d\u6587.zip"
        result = self.run_pack("--input", str(self.source), "--output-dir", str(self.output), "--name", name)
        self.assertEqual(result.returncode, 0, result.stderr)
        with zipfile.ZipFile(self.output / name) as archive:
            self.assertEqual(set(archive.namelist()), {".empty/", ".hidden", "main.pyi"})
            self.assertEqual(archive.read("main.pyi"), b"stub")
        self.assertEqual((self.source / ".hidden").read_text(), "hidden")
        result = self.run_pack(environment={"MCDH_INPUT_DIR": str(self.source), "MCDH_OUTPUT_DIR": str(self.output)})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.output / "custom-export.zip").is_file())

    def test_rejects_traversal_and_output_inside_input(self):
        result = self.run_pack("--input", str(self.source), "--output-dir", str(self.output), "--name", "../escape.zip")
        self.assertNotEqual(result.returncode, 0)
        result = self.run_pack("--input", str(self.source), "--output-dir", str(self.source))
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(list(self.output.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
