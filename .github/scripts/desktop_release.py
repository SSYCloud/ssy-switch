"""Publish complete desktop releases; resume Apple submissions without resubmitting."""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import uuid
from build_version import release_version


MARKER = "ssy-desktop-release"
WORKFLOW = ".github/workflows/desktop-release.yml"
MAC_ARTIFACT = "ssy-switch-macos-universal-not-notarized"
WINDOWS_ARTIFACT = "ssy-switch-windows-x64"


def command(args, *, input_text=None, timeout=180, env=None, sensitive=False):
    try:
        result = subprocess.run(
            [str(arg) for arg in args], input=input_text, capture_output=True,
            text=True, timeout=timeout, env=env,
        )
    except subprocess.TimeoutExpired:
        # Do not include the command line: Apple credentials are arguments.
        raise RuntimeError(f"{args[0]} operation timed out") from None
    if result.returncode:
        if not sensitive:
            print(result.stdout + result.stderr, flush=True)
        raise RuntimeError(f"{args[0]} operation failed (exit {result.returncode})")
    return result.stdout + result.stderr if args[0] == "codesign" else result.stdout


class GitHub:
    def __init__(self, repository):
        self.repository = repository

    def api(self, path, *, method="GET", body=None):
        args = ["gh", "api", f"repos/{self.repository}/{path}", "--method", method]
        if body is not None:
            args.extend(["--input", "-"])
        raw = command(args, input_text=None if body is None else json.dumps(body))
        return json.loads(raw) if raw.strip() else None

    def releases(self):
        result = []
        page = 1
        while True:
            batch = self.api(f"releases?per_page=100&page={page}")
            result.extend(batch)
            if len(batch) < 100:
                return result
            page += 1

    def download(self, run_id, name, destination):
        destination.mkdir(parents=True, exist_ok=True)
        command([
            "gh", "run", "download", str(run_id), "--repo", self.repository,
            "--name", name, "--dir", destination,
        ], timeout=300)


def metadata(release):
    match = re.search(r"<!-- " + MARKER + r"\n(.*?)\n-->", release.get("body") or "", re.S)
    if not match:
        return None
    data = json.loads(match.group(1))
    if data.get("schema") not in (1, 2):
        raise ValueError("Unsupported desktop release metadata")
    data["run_id"] = int(data["run_id"])
    if data.get("state") not in ("pending", "published", "rejected"):
        raise ValueError("Invalid desktop release state")
    # Published releases do not expose Apple's submission identifier. Continue
    # accepting older records while pending drafts still require it for recovery.
    if data["state"] != "published" or "submission_id" in data:
        data["submission_id"] = str(uuid.UUID(data["submission_id"]))
    if data["run_id"] <= 0 or not re.fullmatch(r"[0-9a-f]{40}", data["sha"]):
        raise ValueError("Invalid desktop release source")
    if not re.fullmatch(r"[0-9A-Za-z.+-]+", data["version"]):
        raise ValueError("Invalid desktop release version")
    return data


def tag_for(data):
    return f"v{data['version']}-build.{data['run_id']}"


def notes(repository, data):
    # Allowlist public bookkeeping fields; never serialize the full job state.
    saved = {key: data[key] for key in ("schema", "run_id", "sha", "version", "state")}
    if data["state"] != "published":
        saved["submission_id"] = data["submission_id"]
        if "changes" in data:
            saved["changes"] = data["changes"]
    changes = data.get("changes", "## 更新内容\n\n维护更新。")
    return (
        f"{changes.strip()}\n\n"
        "## 下载\n\n"
        "- **Windows 64 位**：下载下方的 `x64-setup.exe` 安装包。\n"
        "- **macOS**：下载下方的 `universal.dmg`，同时支持 Intel 和 Apple 芯片。\n\n"
        f"<!-- {MARKER}\n{json.dumps(saved, sort_keys=True)}\n-->\n"
    )


def release_changes(github, data):
    previous = []
    for item in github.releases():
        saved = metadata(item)
        if not item["draft"] and not item["prerelease"] and saved and saved["run_id"] < data["run_id"]:
            previous.append(saved)
    baseline = max(previous, key=lambda item: item["run_id"])["sha"] if previous else None
    # Read editorial notes from the source being released, never a newer main.
    entries = github.api(f"contents/.github?ref={data['sha']}")
    if any(item["name"] == "release-notes.json" for item in entries):
        source = github.api(f"contents/.github/release-notes.json?ref={data['sha']}")
        editorial = json.loads(base64.b64decode(source["content"]))
        if editorial.get("base_sha") == baseline:
            body = editorial.get("body")
            if not isinstance(body, str) or not body.strip():
                raise ValueError("Release notes must describe the user-facing changes")
            return body.strip()
    if not baseline:
        return "## 更新内容\n\n首次发布 SSY-Switch 桌面版。"
    # Do not repeat last release's highlights when its editorial file is stale.
    groups = {"feat": [], "fix": [], "perf": [], "other": []}
    page = 1
    while True:
        comparison = github.api(f"compare/{baseline}...{data['sha']}?per_page=100&page={page}")
        commits = comparison["commits"]
        for commit in commits:
            subject = commit["commit"]["message"].splitlines()[0]
            parsed = re.match(r"^(\w+)(?:\(([^)]+)\))?!?:\s*(.+)$", subject)
            if parsed:
                kind, scope, text = parsed.groups()
                if kind not in ("feat", "fix", "perf", "revert") or scope in ("ci", "build", "release", "deps"):
                    continue
                groups[kind if kind in groups else "other"].append(text)
            elif not re.match(r"(?i)^(merge\b|ci(?:[- :]|$)|build\b|docs\b|test\b|chore\b)", subject):
                groups["other"].append(subject)
        if len(commits) < 100:
            break
        page += 1
    sections = []
    for key, heading in (("feat", "新增"), ("fix", "修复"), ("perf", "优化"), ("other", "其他更新")):
        if groups[key]:
            sections.append(f"## {heading}\n\n" + "\n".join(f"- {text}" for text in dict.fromkeys(groups[key])))
    return "\n\n".join(sections) or "## 更新内容\n\n维护更新。"


def summary(message):
    print(message, flush=True)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as output:
            output.write(message + "\n\n")


def source_run(github, run_id, expected_sha=None):
    run = github.api(f"actions/runs/{run_id}")
    if (
        run["path"] != WORKFLOW or run["head_branch"] != "main"
        or run["event"] not in ("push", "workflow_dispatch")
        or run["head_repository"]["full_name"] != github.repository
        or (expected_sha is not None and run["head_sha"] != expected_sha)
    ):
        raise ValueError("Release artifacts must come from this repository's main release workflow")
    return run


def require_artifacts(github, run_id):
    artifacts = github.api(f"actions/runs/{run_id}/artifacts?per_page=100")["artifacts"]
    for name in (MAC_ARTIFACT, WINDOWS_ARTIFACT, "macos-notary-submission"):
        matches = [a for a in artifacts if a["name"] == name and not a["expired"]]
        if len(matches) != 1:
            raise ValueError(f"Expected one unexpired {name} artifact in run {run_id}")


def checksum_files(folder):
    expected = {}
    for line in (folder / "SHA256SUMS.txt").read_text().splitlines():
        match = re.fullmatch(r"([0-9a-fA-F]{64}) [ *](.+)", line)
        if not match:
            raise ValueError("Malformed SHA256SUMS.txt")
        digest, name = match.groups()
        if Path(name).name != name or "/" in name or "\\" in name or name in expected:
            raise ValueError("Invalid or duplicate checksum filename")
        expected[name] = digest.lower()
    actual = {p.name for p in folder.iterdir() if p.name != "SHA256SUMS.txt"}
    if not expected or actual != set(expected):
        raise ValueError("Package files do not match the checksum manifest")
    for name, digest in expected.items():
        path = folder / name
        if path.is_symlink() or not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise ValueError(f"Checksum failed: {name}")


def exactly_one(folder, pattern):
    matches = list(folder.glob(pattern))
    if len(matches) != 1:
        raise ValueError(f"Expected exactly one {pattern} in {folder.name}")
    return matches[0]


def apple(args):
    auth = []
    for name, option in (("APPLE_ID", "--apple-id"), ("APPLE_PASSWORD", "--password"), ("APPLE_TEAM_ID", "--team-id")):
        if not os.environ.get(name):
            raise ValueError(f"Missing {name}")
        auth.extend([option, os.environ[name]])
    return command(["xcrun", "notarytool", *args, *auth])


def require_ticket_hashes(hashes, log):
    accepted = {(t.get("arch"), t.get("cdhash")) for t in log.get("ticketContents", []) if t.get("path", "").endswith(".app")}
    if set(hashes) != {"x86_64", "arm64"} or not all((arch, digest) in accepted for arch, digest in hashes.items()):
        raise ValueError("Package code signatures differ from the accepted Apple submission")


def prepare_assets(github, data, folder, log):
    mac, windows, assets = folder / "mac", folder / "windows", folder / "assets"
    github.download(data["run_id"], MAC_ARTIFACT, mac)
    github.download(data["run_id"], WINDOWS_ARTIFACT, windows)
    checksum_files(mac)
    checksum_files(windows)
    app_zip = exactly_one(mac, "*.zip")
    dmg = exactly_one(mac, "*.dmg")
    exe = exactly_one(windows, "*.exe")
    msi = exactly_one(windows, "*.msi")
    if not log.get("archiveFilename", "").endswith(".dmg"):
        raise ValueError("This submission does not cover the distributable DMG")
    command(["ditto", "-x", "-k", app_zip, folder / "unpacked"])
    app = exactly_one(folder / "unpacked", "*.app")
    if data["schema"] == 2:
        version = command(["/usr/libexec/PlistBuddy", "-c", "Print :CFBundleShortVersionString", app / "Contents/Info.plist"]).strip()
        if version != data["version"] or any(f"_{version}_" not in p.name for p in (exe, msi, dmg)):
            raise ValueError("Installer version does not match this release")
    command(["codesign", "--verify", "--deep", "--strict", "--verbose=2", app])
    hashes = {}
    for arch in ("x86_64", "arm64"):
        details = command(["codesign", "--display", "--verbose=4", "--arch", arch, app])
        if "TeamIdentifier=" + os.environ["APPLE_TEAM_ID"] not in details.splitlines():
            raise ValueError("Unexpected Developer ID team")
        match = re.search(r"^CDHash=([0-9a-f]+)$", details, re.M)
        if not match:
            raise ValueError(f"Missing code signature for {arch}")
        hashes[arch] = match.group(1)
    require_ticket_hashes(hashes, log)
    command(["codesign", "--verify", "--verbose=2", dmg])
    details = command(["codesign", "--display", "--verbose=4", dmg])
    if "TeamIdentifier=" + os.environ["APPLE_TEAM_ID"] not in details.splitlines():
        raise ValueError("Unexpected DMG signing team")
    for path in (app, dmg):
        command(["xcrun", "stapler", "staple", path])
        command(["xcrun", "stapler", "validate", path])
    command(["spctl", "--assess", "--type", "execute", "--verbose=2", app])
    command(["spctl", "--assess", "--type", "open", "--context", "context:primary-signature", "--verbose=2", dmg])
    assets.mkdir()
    for path in (exe, msi):
        shutil.copy2(path, assets / path.name)
    shutil.copy2(dmg, assets / dmg.name.replace("-not-notarized.dmg", ".dmg"))
    command(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", app, assets / "SSY-Switch-macOS-universal.zip"])
    if data["schema"] == 2:
        from updater_assets import prepare_updates
        config = source_config(github, data["sha"])
        prepare_updates(command, assets, app, github.repository, tag_for(data), data["version"], config["plugins"]["updater"]["pubkey"])
    lines = [hashlib.sha256(p.read_bytes()).hexdigest() + "  " + p.name for p in sorted(assets.iterdir())]
    (assets / "SHA256SUMS.txt").write_text("\n".join(lines) + "\n")
    return assets


def should_be_latest(data, releases):
    for release in releases:
        other = metadata(release)
        if not release["draft"] and not release["prerelease"] and other and other["run_id"] > data["run_id"]:
            return False
    return True


def finish(github, release, data, folder):
    if release["tag_name"] != tag_for(data):
        raise ValueError("Release tag does not match its source metadata")
    source_run(github, data["run_id"], data["sha"])
    require_artifacts(github, data["run_id"])
    decision = json.loads(apple(["info", data["submission_id"], "--output-format", "json"]))
    status = decision["status"]
    if status == "In Progress":
        summary(f"Apple is still processing `{data['submission_id']}`. Draft retained: {release['html_url']}. The scheduled workflow will resume this submission.")
        return
    log_path = folder / "notary-log.json"
    apple(["log", data["submission_id"], str(log_path)])
    log = json.loads(log_path.read_text())
    if status != "Accepted":
        data = dict(data, state="rejected")
        github.api(f"releases/{release['id']}", method="PATCH", body={"body": notes(github.repository, data)})
        print(json.dumps(log, ensure_ascii=False, indent=2))
        raise ValueError(f"Apple rejected {data['submission_id']}: {status}; draft was not published")
    if log.get("jobId") != data["submission_id"] or log.get("status") != "Accepted":
        raise ValueError("Apple log does not match the accepted submission")
    if "changes" not in data:
        data = dict(data, changes=release_changes(github, data))
    assets = prepare_assets(github, data, folder, log)
    # Only drafts are mutable. A retry can replace partial uploads before publish.
    current = github.api(f"releases/{release['id']}")
    if not current["draft"]:
        summary(f"Already published: {current['html_url']}")
        return
    command(["gh", "release", "upload", release["tag_name"], *sorted(assets.iterdir()), "--repo", github.repository, "--clobber"], timeout=300)
    uploaded = github.api(f"releases/{release['id']}")["assets"]
    expected = {p.name: p.stat().st_size for p in assets.iterdir()}
    actual = {a["name"]: a["size"] for a in uploaded if a["state"] == "uploaded"}
    if actual != expected:
        raise ValueError("Release uploads are incomplete or contain unexpected files; draft retained")
    data = dict(data, state="published")
    result = github.api(f"releases/{release['id']}", method="PATCH", body={
        "draft": False, "prerelease": False, "name": f"SSY-Switch {data['version']}",
        "body": notes(github.repository, data),
        "make_latest": "true" if should_be_latest(data, github.releases()) else "false",
    })
    summary(f"Published Windows installers and notarized macOS DMG: {result['html_url']}")


def start(github, run_id, folder):
    run = source_run(github, run_id)
    require_artifacts(github, run_id)
    state = folder / "submission"
    github.download(run_id, "macos-notary-submission", state)
    submission = str(uuid.UUID(json.loads((state / "submission.json").read_text())["id"]))
    # Read version from the source commit, including when resumed on a newer main.
    config = source_config(github, run["head_sha"])
    updater_enabled = bool(config.get("plugins", {}).get("updater", {}).get("pubkey"))
    version = release_version(config["version"], run["run_number"]) if updater_enabled else config["version"]
    data = {"schema": 2 if updater_enabled else 1, "run_id": run_id, "sha": run["head_sha"], "version": version, "submission_id": submission, "state": "pending"}
    existing = next((r for r in github.releases() if r["tag_name"] == tag_for(data)), None)
    if existing:
        saved = metadata(existing)
        if not saved or any(saved[k] != data[k] for k in ("run_id", "sha", "version")):
            raise ValueError("Existing release belongs to different build artifacts")
        if not existing["draft"]:
            summary(f"Already published: {existing['html_url']}")
            return
        if saved.get("submission_id") != submission:
            raise ValueError("Existing release belongs to different build artifacts")
        data["changes"] = saved.get("changes") or release_changes(github, data)
        release = existing
    else:
        data["changes"] = release_changes(github, data)
        release = github.api("releases", method="POST", body={
            "tag_name": tag_for(data), "target_commitish": data["sha"],
            "name": f"SSY-Switch {version}",
            "body": notes(github.repository, data), "draft": True, "prerelease": False,
        })
    finish(github, release, data, folder)


def source_config(github, sha):
    import base64
    config = github.api(f"contents/src-tauri/tauri.conf.json?ref={sha}")
    return json.loads(base64.b64decode(config["content"]))


def resume(github):
    pending = []
    for release in github.releases():
        data = metadata(release)
        if release["draft"] and data and data["state"] == "pending":
            pending.append((release, data))
    if not pending:
        summary("No desktop releases are awaiting Apple notarization.")
        return
    failures = []
    for release, data in sorted(pending, key=lambda item: item[1]["run_id"])[:10]:
        try:
            with tempfile.TemporaryDirectory(prefix="ssy-release-") as folder:
                finish(github, release, data, Path(folder))
        except (RuntimeError, ValueError, KeyError, OSError) as error:
            failures.append(data["run_id"])
            print(f"::error::Release {data['run_id']}: {error}", flush=True)
    if failures:
        raise RuntimeError(f"Some releases need attention: {failures}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--run-id", type=int)
    mode.add_argument("--pending", action="store_true")
    args = parser.parse_args()
    github = GitHub(os.environ["GITHUB_REPOSITORY"])
    if args.pending:
        resume(github)
    else:
        with tempfile.TemporaryDirectory(prefix="ssy-release-") as folder:
            start(github, args.run_id, Path(folder))


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, ValueError, KeyError, OSError) as error:
        print(f"::error::{error}", flush=True)
        raise SystemExit(1)
