import contextlib
import io
import sys
import unittest

from private_command import run


class PrivateCommandTests(unittest.TestCase):
    def test_success_does_not_publish_source_or_stderr(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            status = run([sys.executable, "-c", "import sys; print('PRIVATE_SOURCE'); print('PRIVATE_DETAIL', file=sys.stderr)"])
        self.assertEqual(status, 0)
        self.assertNotIn("PRIVATE_SOURCE", output.getvalue())
        self.assertNotIn("PRIVATE_DETAIL", output.getvalue())
        self.assertIn("passed", output.getvalue())

    def test_failure_preserves_status_without_diagnostics(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            status = run([sys.executable, "-c", "print('PRIVATE_SOURCE'); raise SystemExit(7)"])
        self.assertEqual(status, 7)
        self.assertNotIn("PRIVATE_SOURCE", output.getvalue())
        self.assertIn("failed", output.getvalue())

    def test_missing_command_fails_closed(self):
        with self.assertRaises(ValueError):
            run([])
