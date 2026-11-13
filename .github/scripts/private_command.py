"""Run checks without publishing diagnostics that may contain private source."""

import subprocess
import re
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
        log.seek(0)
        for raw in log:
            line = raw.decode("utf-8", errors="replace").strip()
            # Only public test identifiers and counts are safe to report;
            # never print assertion bodies, URLs, source excerpts or payloads.
            if re.fullmatch(r"test [A-Za-z0-9_:]+ \.\.\. FAILED", line):
                print(line)
            elif re.fullmatch(r"test result: (?:ok|FAILED)\. \d+ passed; \d+ failed; \d+ ignored; \d+ measured; \d+ filtered out; finished in [0-9.]+s", line):
                print(line)
            elif match := re.match(r"error\[(E\d{4})\]:", line):
                print("Compiler diagnostic code:", match.group(1))
    print("Private check passed." if result.returncode == 0 else
          "Private check failed; diagnostics were not published.")
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(run(sys.argv[1:]))
