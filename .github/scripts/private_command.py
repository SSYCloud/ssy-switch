"""Run checks without publishing diagnostics that may contain private source."""

import subprocess
import sys
import tempfile


def run(args):
    if not args:
        raise ValueError("A command is required")
    # The temporary log is never uploaded and is removed on both success and
    # failure. Detailed SDK diagnostics belong in the private SDK repository.
    with tempfile.TemporaryFile() as log:
        try:
            result = subprocess.run(args, stdout=log, stderr=log)
        except OSError:
            print("Private check could not start; diagnostics were not published.")
            return 1
    print("Private check passed." if result.returncode == 0 else
          "Private check failed; diagnostics were not published.")
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(run(sys.argv[1:]))
