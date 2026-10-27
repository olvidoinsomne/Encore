import hashlib
import importlib.util
import io
import json
from pathlib import Path
import unittest
import urllib.error
from unittest.mock import Mock

spec = importlib.util.spec_from_file_location("mirror", Path(__file__).resolve().parents[1] / "scripts/mirror-github-releases.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class MirrorTests(unittest.TestCase):
    def setUp(self):
        self.mirror = module.Mirror.__new__(module.Mirror)
        self.mirror.gitlab = "https://gitlab.example"
        self.mirror.project = "26"
        self.mirror.github = "https://api.github.com/repos/olvidoinsomne/Encore"
        self.mirror.uploads = "https://uploads.github.com/repos/olvidoinsomne/Encore"
        self.source = {"tag_name": "v1", "name": "Encore 1", "commit": {"id": "abc"},
                       "assets": {"links": [{"name": "Encore.zip", "link_type": "package", "url": "https://gitlab.example/a/-/jobs/12/artifacts/raw/dist/Encore.zip"}]}}

    def test_tag_mismatch_stops_publication(self):
        self.mirror.request = Mock(return_value={"object": {"type": "commit", "sha": "wrong"}})
        with self.assertRaisesRegex(ValueError, "differs"):
            self.mirror.sync(self.source)
        self.assertEqual(self.mirror.request.call_count, 1)

    def test_annotated_tag_is_peeled(self):
        self.mirror.request = Mock(side_effect=[{"object": {"type": "tag", "sha": "tag"}}, {"object": {"type": "commit", "sha": "abc"}}])
        self.mirror.verify_tag(self.source)
        self.assertEqual(self.mirror.request.call_count, 2)

    def prepare_sync(self, existing):
        self.mirror.verify_tag = Mock()
        self.mirror.pages = Mock(side_effect=[iter([{"id": 3, "tag_name": "v1", "draft": True}]), iter(existing)])
        self.mirror.download = Mock(return_value=b"package")
        self.mirror.request = Mock(return_value={"id": 5, "state": "uploaded", "size": 7, "digest": "sha256:" + hashlib.sha256(b"package").hexdigest()})

    def test_retry_resumes_draft_and_skips_verified_asset(self):
        self.prepare_sync([{"id": 5, "name": "Encore.zip", "state": "uploaded", "size": 7, "digest": "sha256:" + hashlib.sha256(b"package").hexdigest()}])
        self.mirror.sync(self.source)
        self.assertEqual(self.mirror.request.call_count, 1)
        self.assertEqual(self.mirror.request.call_args.args[1], "PATCH")
        self.assertFalse(self.mirror.request.call_args.args[2]["draft"])

    def test_conflicting_asset_is_never_overwritten(self):
        self.prepare_sync([{"id": 5, "name": "Encore.zip", "state": "uploaded", "size": 7, "digest": "sha256:wrong"}])
        with self.assertRaisesRegex(ValueError, "refusing to overwrite"):
            self.mirror.sync(self.source)
        self.mirror.request.assert_not_called()

    def test_upload_finishes_before_publication(self):
        self.prepare_sync([])
        self.mirror.sync(self.source)
        self.assertEqual(self.mirror.request.call_args_list[0].args[1:3], ("POST", b"package"))
        self.assertEqual(self.mirror.request.call_args_list[1].args[1], "PATCH")

    def test_artifact_url_becomes_job_token_api_route(self):
        self.assertEqual(self.mirror.source_asset_url(self.source["assets"]["links"][0]["url"]),
                         "https://gitlab.example/api/v4/projects/26/jobs/12/artifacts/dist/Encore.zip")
        with self.assertRaises(ValueError):
            self.mirror.source_asset_url("https://elsewhere.example/file")

    def test_upload_uses_binary_body_and_json_response(self):
        self.mirror.token = "github-secret"
        self.mirror.job_token = "gitlab-secret"
        self.mirror.opener = Mock()
        response = io.BytesIO(json.dumps({"id": 5}).encode())
        self.mirror.opener.open.return_value = response
        self.assertEqual(self.mirror.request(self.mirror.uploads + "/releases/3/assets", "POST", b"package"), {"id": 5})
        request = self.mirror.opener.open.call_args.args[0]
        self.assertEqual(request.data, b"package")
        self.assertEqual(request.get_header("Content-type"), "application/octet-stream")
        self.assertEqual(request.get_header("Authorization"), "Bearer github-secret")
        self.assertIsNone(request.get_header("Job-token"))

    def test_redirect_rebuilds_credentials_for_storage(self):
        self.mirror.token = "github-secret"
        self.mirror.job_token = "gitlab-secret"
        self.mirror.opener = Mock()
        self.mirror.opener.open.side_effect = [urllib.error.HTTPError("https://gitlab.example/file", 302, "redirect", {"Location": "https://storage.example/signed"}, None), io.BytesIO(b"package")]
        self.assertEqual(self.mirror.download("https://gitlab.example/file"), b"package")
        first, second = [c.args[0] for c in self.mirror.opener.open.call_args_list]
        self.assertEqual(first.get_header("Job-token"), "gitlab-secret")
        self.assertIsNone(second.get_header("Job-token"))
        self.assertIsNone(second.get_header("Authorization"))


if __name__ == "__main__":
    unittest.main()
