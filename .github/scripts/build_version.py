"""Give every automatic build a sortable SemVer that also fits Windows MSI."""
import argparse
import json
from pathlib import Path
import re


def release_version(base, run_number):
    match = re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", base)
    if not match or not isinstance(run_number, int) or isinstance(run_number, bool) or run_number < 1:
        raise ValueError("A stable base version and positive Actions run number are required")
    major, minor, patch = map(int, match.groups())
    patch += run_number
    if major > 255 or minor > 255 or patch > 65535:
        raise ValueError("Release version exceeds Windows MSI limits; advance the minor version")
    return f"{major}.{minor}.{patch}"


def prepare(root, run_number):
    config_path = root / "src-tauri/tauri.conf.json"
    config = json.loads(config_path.read_text(encoding="utf-8"))
    version = release_version(config["version"], run_number)
    for path in (config_path, root / "package.json"):
        value = json.loads(path.read_text(encoding="utf-8"))
        value["version"] = version
        path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    cargo = root / "src-tauri/Cargo.toml"
    cargo.write_text(re.sub(r'(?m)^version = "[^"]+"', f'version = "{version}"', cargo.read_text(encoding="utf-8"), count=1), encoding="utf-8")
    for lock in (root / "Cargo.lock", root / "src-tauri/Cargo.lock"):
        if lock.exists():
            lock.write_text(re.sub(r'(name = "ssy-switch"\nversion = ")[^"]+', lambda m: m[1] + version, lock.read_text(encoding="utf-8")), encoding="utf-8")
    print("Build version:", version)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-number", type=int, required=True)
    args = parser.parse_args()
    prepare(Path(__file__).resolve().parents[2], args.run_number)
