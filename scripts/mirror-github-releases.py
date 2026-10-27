"""Copy GitLab releases and their binary packages to the GitHub mirror.

GitLab remains authoritative. Git tags must already exist on GitHub; this
script never creates/moves tags or deletes releases/assets. Standard library only.
"""
import datetime as dt
import hashlib
import json
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class Mirror:
    def __init__(self):
        self.gitlab = os.environ.get("CI_SERVER_URL", "https://gitlab.timajones.com").rstrip("/")
        self.project = os.environ.get("CI_PROJECT_ID", "26")
        self.github = "https://api.github.com/repos/olvidoinsomne/Encore"
        self.uploads = "https://uploads.github.com/repos/olvidoinsomne/Encore"
        self.token = os.environ["GITHUB_RELEASE_TOKEN"]
        self.job_token = os.environ["CI_JOB_TOKEN"]
        self.opener = urllib.request.build_opener(NoRedirect)

    def request(self, url, method="GET", payload=None, binary=False):
        headers = {"User-Agent": "Encore-GitLab-release-mirror"}
        if url.startswith(self.github + "/") or url.startswith(self.uploads + "/"):
            headers.update(Authorization="Bearer " + self.token,
                           Accept="application/vnd.github+json",
                           **{"X-GitHub-Api-Version": "2026-03-10"})
        elif url.startswith(self.gitlab + "/"):
            headers["JOB-TOKEN"] = self.job_token
        data = payload
        if payload is not None:
            headers["Content-Type"] = "application/octet-stream" if isinstance(payload, bytes) else "application/json"
            if not isinstance(payload, bytes):
                data = json.dumps(payload).encode()
        response = self.opener.open(urllib.request.Request(url, data=data, headers=headers, method=method), timeout=180)
        with response:
            body = response.read()
        return body if binary else json.loads(body)

    def pages(self, url):
        page = 1
        while True:
            separator = "&" if "?" in url else "?"
            items = self.request(f"{url}{separator}per_page=100&page={page}")
            yield from items
            if len(items) < 100:
                return
            page += 1

    def download(self, url):
        # Release links must originate on GitLab. Rebuild credentials for each
        # redirect so JOB-TOKEN cannot leak to an object storage/CDN destination.
        if not url.startswith(self.gitlab + "/"):
            raise ValueError("Release asset must be hosted by the primary GitLab instance")
        for _ in range(6):
            if urllib.parse.urlsplit(url).scheme != "https":
                raise ValueError("Release asset redirects must use HTTPS")
            try:
                return self.request(url, binary=True)
            except urllib.error.HTTPError as error:
                if error.code not in (301, 302, 303, 307, 308) or not error.headers.get("Location"):
                    raise
                url = urllib.parse.urljoin(url, error.headers["Location"])
        raise ValueError("Too many asset redirects")

    def source_asset_url(self, url):
        # GitLab web artifact routes can require an interactive session. The
        # equivalent API route accepts this job's ephemeral CI_JOB_TOKEN.
        parts = urllib.parse.urlsplit(url)
        if parts.scheme != "https" or parts.netloc != urllib.parse.urlsplit(self.gitlab).netloc:
            raise ValueError("Release asset must be hosted by the primary GitLab instance")
        match = re.fullmatch(r"/.+/-/jobs/(\d+)/artifacts/raw/(.+)", parts.path)
        if match:
            return f"{self.gitlab}/api/v4/projects/{self.project}/jobs/{match[1]}/artifacts/{match[2]}"
        return url

    def verify_tag(self, release):
        tag = urllib.parse.quote(release["tag_name"], safe="")
        ref = self.request(f"{self.github}/git/ref/tags/{tag}")["object"]
        for _ in range(8):
            if ref["type"] == "commit":
                if ref["sha"] != release["commit"]["id"]:
                    raise ValueError(f"GitHub tag {release['tag_name']} differs from GitLab")
                return
            if ref["type"] != "tag":
                break
            ref = self.request(f"{self.github}/git/tags/{ref['sha']}")["object"]
        raise ValueError("GitHub tag does not resolve to a commit")

    def sync(self, source):
        tag = source["tag_name"]
        # Future scheduled GitLab releases must not be published early.
        released = source.get("released_at")
        if released and dt.datetime.fromisoformat(released.replace("Z", "+00:00")) > dt.datetime.now(dt.timezone.utc):
            print(f"Skip future release {tag}")
            return
        self.verify_tag(source)
        # Draft releases are omitted by the tag lookup API, so list them too.
        target = next((r for r in self.pages(self.github + "/releases") if r["tag_name"] == tag), None)
        body = source.get("description") or ""
        source_url = source.get("_links", {}).get("self") or f"{self.gitlab}/shared/Encore/-/releases/{urllib.parse.quote(tag, safe='')}"
        body += f"\n\nMirrored from [the primary GitLab release]({source_url}).\n"
        fields = {"name": source.get("name") or tag, "body": body}
        if target is None:
            target = self.request(self.github + "/releases", "POST", dict(fields, tag_name=tag, draft=True))
        assets = {a["name"]: a for a in self.pages(f"{self.github}/releases/{target['id']}/assets")}
        for link in source.get("assets", {}).get("links", []):
            name = link["name"]
            if link.get("link_type") != "package":
                continue
            package = self.download(self.source_asset_url(link["url"]))
            digest = "sha256:" + hashlib.sha256(package).hexdigest()
            if name in assets:
                old = assets[name]
                if old.get("state") != "uploaded" or old["size"] != len(package) or old.get("digest") not in (None, digest):
                    raise ValueError(f"Existing GitHub asset {name} differs; refusing to overwrite it")
                if old.get("digest") is None:
                    # Require a digest rather than accepting a legacy asset
                    # based on its size alone.
                    raise ValueError(f"Existing asset {name} has no digest; manual verification required")
                print(f"Verified {tag}: {name}")
                continue
            uploaded = self.request(f"{self.uploads}/releases/{target['id']}/assets?name={urllib.parse.quote(name, safe='')}", "POST", package)
            if uploaded.get("state") != "uploaded" or uploaded.get("size") != len(package) or uploaded.get("digest") != digest:
                raise ValueError(f"Uploaded asset {name} failed digest verification")
            print(f"Uploaded {tag}: {name}")
            assets[name] = uploaded
        self.request(f"{self.github}/releases/{target['id']}", "PATCH", dict(fields, draft=False))
        print(f"Mirrored {tag}")

    def run(self):
        failures = []
        for release in self.pages(f"{self.gitlab}/api/v4/projects/{self.project}/releases"):
            try:
                self.sync(release)
            except (urllib.error.HTTPError, ValueError) as error:
                # Never log request headers, tokens or arbitrary server bodies.
                detail = f"HTTP {error.code}" if isinstance(error, urllib.error.HTTPError) else str(error)
                print(f"Failed {release['tag_name']}: {detail}", file=sys.stderr)
                failures.append(release["tag_name"])
        if failures:
            raise SystemExit(1)


if __name__ == "__main__":
    Mirror().run()
