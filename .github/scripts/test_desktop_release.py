import base64
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

import desktop_release as release


DATA = {
    "schema": 1, "run_id": 100, "sha": "a" * 40, "version": "3.20.2",
    "submission_id": "d6370cdc-5cd8-4c6f-8264-005905735344", "state": "pending",
}


def draft(data=DATA):
    return {
        "id": 5, "tag_name": release.tag_for(data), "draft": True,
        "prerelease": False, "html_url": "https://github.com/example/repo/releases/5",
        "body": release.notes("example/repo", data),
    }


class IntegrityTests(unittest.TestCase):
    def test_metadata_roundtrip(self):
        self.assertEqual(release.metadata(draft()), DATA)
        self.assertIsNone(release.metadata({"body": "An unrelated release"}))

    def test_published_notes_omit_submission_and_unlisted_private_fields(self):
        data = dict(DATA, state="published", changes="## 新增\n\n- 应用内自动更新。", password="do-not-publish")
        body = release.notes("example/repo", data)
        self.assertIn("应用内自动更新", body)
        for private in (DATA["submission_id"], "submission_id", "password", "do-not-publish", "actions/runs/", "Apple 公证编号"):
            self.assertNotIn(private, body)
        parsed = release.metadata({"body": body})
        self.assertEqual(parsed, {key: data[key] for key in ("schema", "run_id", "sha", "version", "state")})

    def test_pending_draft_retains_recovery_state_but_not_extra_fields(self):
        data = dict(DATA, changes="本版新增更新功能。", password="do-not-publish")
        parsed = release.metadata({"body": release.notes("example/repo", data)})
        self.assertEqual(parsed["submission_id"], DATA["submission_id"])
        self.assertEqual(parsed["changes"], data["changes"])
        self.assertNotIn("password", parsed)

    def test_legacy_published_metadata_still_loads(self):
        data = dict(DATA, state="published")
        body = f"<!-- {release.MARKER}\n{json.dumps(data)}\n-->"
        self.assertEqual(release.metadata({"body": body}), data)

    def test_pending_metadata_without_submission_is_rejected(self):
        data = {key: value for key, value in DATA.items() if key != "submission_id"}
        body = f"<!-- {release.MARKER}\n{json.dumps(data)}\n-->"
        with self.assertRaises(KeyError):
            release.metadata({"body": body})

    def test_manifest_checks_content_and_unlisted_files(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            package = root / "app.dmg"
            package.write_bytes(b"signed app")
            digest = hashlib.sha256(package.read_bytes()).hexdigest()
            (root / "SHA256SUMS.txt").write_text(digest + "  app.dmg\n")
            release.checksum_files(root)
            package.write_bytes(b"changed app")
            with self.assertRaisesRegex(ValueError, "Checksum failed"):
                release.checksum_files(root)
            package.write_bytes(b"signed app")
            (root / "unexpected.exe").write_bytes(b"extra")
            with self.assertRaisesRegex(ValueError, "do not match"):
                release.checksum_files(root)

    def test_manifest_rejects_path_escape(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "SHA256SUMS.txt").write_text("a" * 64 + "  ../other.dmg\n")
            with self.assertRaisesRegex(ValueError, "filename"):
                release.checksum_files(root)

    def test_ticket_requires_both_matching_architectures(self):
        hashes = {"arm64": "arm", "x86_64": "intel"}
        log = {"ticketContents": [{"arch": a, "cdhash": h, "path": "app.dmg/SSY-Switch.app"} for a, h in hashes.items()]}
        release.require_ticket_hashes(hashes, log)
        for changed in ({"arm64": "arm"}, {"arm64": "new", "x86_64": "intel"}):
            with self.assertRaisesRegex(ValueError, "differ"):
                release.require_ticket_hashes(changed, log)

    def test_source_rejects_other_branches_repos_workflows_and_commits(self):
        github = Mock(repository="example/repo")
        good = {"path": release.WORKFLOW, "head_branch": "main", "event": "push", "head_sha": DATA["sha"], "head_repository": {"full_name": "example/repo"}}
        github.api.return_value = good
        self.assertEqual(release.source_run(github, 100, DATA["sha"]), good)
        for changed in (
            {"head_branch": "untrusted"}, {"event": "pull_request"},
            {"head_repository": {"full_name": "someone/fork"}},
            {"path": ".github/workflows/other.yml"}, {"head_sha": "b" * 40},
        ):
            github.api.return_value = dict(good, **changed)
            with self.assertRaisesRegex(ValueError, "main release workflow"):
                release.source_run(github, 100, DATA["sha"])

    def test_old_build_cannot_replace_newer_latest(self):
        newer = dict(DATA, run_id=101, state="published")
        published = dict(draft(newer), draft=False)
        self.assertFalse(release.should_be_latest(DATA, [published]))
        self.assertTrue(release.should_be_latest(newer, [dict(draft(), draft=False)]))


class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.folder = Path(self.temp.name)
        self.github = Mock(repository="example/repo")
        self.github.releases.return_value = []
        self.source = patch.object(release, "source_run").start()
        self.require = patch.object(release, "require_artifacts").start()
        self.assets = patch.object(release, "prepare_assets").start()
        self.apple = patch.object(release, "apple").start()
        self.command = patch.object(release, "command").start()
        self.changes = patch.object(release, "release_changes", return_value="## 新增\n\n- 应用内更新。").start()
        patch.object(release, "summary").start()
        self.addCleanup(patch.stopall)

    def decision(self, status):
        def apple(args):
            if args[0] == "info":
                return json.dumps({"status": status})
            self.assertEqual(args[0], "log")
            Path(args[2]).write_text(json.dumps({"jobId": DATA["submission_id"], "status": status}))
            return ""
        self.apple.side_effect = apple

    def test_pending_does_not_publish_or_resubmit(self):
        self.decision("In Progress")
        release.finish(self.github, draft(), DATA, self.folder)
        self.assets.assert_not_called()
        self.command.assert_not_called()
        self.github.api.assert_not_called()
        self.assertEqual(self.apple.call_args_list[0].args[0][0], "info")

    def test_rejection_preserves_unpublished_draft(self):
        self.decision("Invalid")
        with patch("builtins.print"), self.assertRaisesRegex(ValueError, "draft was not published"):
            release.finish(self.github, draft(), DATA, self.folder)
        body = self.github.api.call_args.kwargs["body"]
        self.assertNotIn("draft", body)
        self.assertEqual(release.metadata({"body": body["body"]})["state"], "rejected")
        self.assets.assert_not_called()

    def accepted_assets(self):
        self.decision("Accepted")
        folder = self.folder / "assets"
        folder.mkdir()
        names = ["app.dmg", "app.zip", "app.exe", "app.msi", "SHA256SUMS.txt"]
        for name in names:
            (folder / name).write_bytes(b"verified")
        self.assets.return_value = folder
        return [{"name": name, "size": 8, "state": "uploaded"} for name in names]

    def test_only_complete_verified_upload_is_published(self):
        uploaded = self.accepted_assets()
        self.github.api.side_effect = [draft(), {"assets": uploaded}, {"html_url": draft()["html_url"]}]
        release.finish(self.github, draft(), DATA, self.folder)
        body = self.github.api.call_args.kwargs["body"]
        self.assertFalse(body["draft"])
        self.assertFalse(body["prerelease"])
        self.assertEqual(body["make_latest"], "true")
        self.assertEqual(release.metadata({"body": body["body"]})["state"], "published")
        self.assertNotIn(DATA["submission_id"], body["body"])
        self.assertEqual(body["name"], "SSY-Switch 3.20.2")
        self.assertEqual(self.command.call_args.args[0][:3], ["gh", "release", "upload"])

    def test_partial_upload_never_publishes(self):
        uploaded = self.accepted_assets()
        self.github.api.side_effect = [draft(), {"assets": uploaded[:-1]}]
        with self.assertRaisesRegex(ValueError, "incomplete"):
            release.finish(self.github, draft(), DATA, self.folder)
        self.assertFalse(any(c.kwargs.get("method") == "PATCH" for c in self.github.api.call_args_list))

    def test_failed_signature_checks_never_upload(self):
        self.decision("Accepted")
        self.assets.side_effect = ValueError("Package code signatures differ")
        with self.assertRaisesRegex(ValueError, "signatures differ"):
            release.finish(self.github, draft(), DATA, self.folder)
        self.command.assert_not_called()
        self.github.api.assert_not_called()

    def test_published_release_is_not_overwritten(self):
        self.accepted_assets()
        self.github.api.return_value = dict(draft(), draft=False)
        release.finish(self.github, draft(), DATA, self.folder)
        self.command.assert_not_called()


class ReleaseNotesTests(unittest.TestCase):
    def setUp(self):
        self.github = Mock(repository="example/repo")
        self.github.releases.return_value = [dict(draft(dict(DATA, state="published")), draft=False)]
        self.current = dict(DATA, run_id=101, sha="b" * 40)

    def editorial(self, baseline=DATA["sha"]):
        return {"content": base64.b64encode(json.dumps({"base_sha": baseline, "body": "## 修复\n\n- 修复余额显示。"}).encode()).decode()}

    def test_editorial_notes_are_read_from_the_built_commit(self):
        self.github.api.side_effect = [[{"name": "release-notes.json"}], self.editorial()]
        self.assertIn("修复余额显示", release.release_changes(self.github, self.current))
        for call in self.github.api.call_args_list:
            self.assertIn("ref=" + self.current["sha"], call.args[0])

    def test_stale_notes_are_not_reused_and_build_process_is_filtered(self):
        messages = ["feat: 支持切换账号", "fix(updater): 修复更新下载失败", "ci: notarize macOS", "fix(release): repair release notes", "docs: update guide", "Merge pull request #1", "perf: 加快启动速度"]
        self.github.api.side_effect = [[{"name": "release-notes.json"}], self.editorial("c" * 40), {"commits": [{"commit": {"message": item}} for item in messages]}]
        body = release.release_changes(self.github, self.current)
        self.assertIn("支持切换账号", body)
        self.assertIn("修复更新下载失败", body)
        self.assertIn("加快启动速度", body)
        for omitted in ("修复余额显示", "notarize", "repair release", "update guide", "Merge"):
            self.assertNotIn(omitted, body)

    def test_maintenance_only_release_does_not_invent_features(self):
        self.github.api.side_effect = [[], {"commits": [{"commit": {"message": "ci: update packaging"}}]}]
        self.assertEqual(release.release_changes(self.github, self.current), "## 更新内容\n\n维护更新。")

    def test_newer_releases_are_not_used_as_comparison_baseline(self):
        self.github.releases.return_value.append(dict(draft(dict(DATA, run_id=102, sha="c" * 40, state="published")), draft=False))
        self.github.api.side_effect = [[{"name": "release-notes.json"}], self.editorial()]
        self.assertIn("修复余额显示", release.release_changes(self.github, self.current))


class StartTests(unittest.TestCase):
    def test_retry_published_release_does_not_require_public_submission_id(self):
        github = Mock(repository="example/repo")
        github.releases.return_value = [dict(draft(dict(DATA, state="published")), draft=False)]
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "submission").mkdir()
            (root / "submission/submission.json").write_text(json.dumps({"id": DATA["submission_id"]}))
            with patch.object(release, "source_run", return_value={"head_sha": DATA["sha"], "run_number": 1}), patch.object(release, "source_config", return_value={"version": DATA["version"]}), patch.object(release, "require_artifacts"), patch.object(release, "summary"), patch.object(release, "finish") as finish:
                release.start(github, DATA["run_id"], root)
            finish.assert_not_called()
            github.api.assert_not_called()


if __name__ == "__main__":
    unittest.main()
