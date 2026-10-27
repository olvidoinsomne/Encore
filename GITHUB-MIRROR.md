# Encore GitHub mirror

GitLab is primary: https://gitlab.timajones.com/shared/Encore
GitHub is downstream: https://github.com/olvidoinsomne/Encore

GitLab's native push mirror synchronizes all branches and tags over SSH.
Its generated public key must be added to GitHub as an authentication key
(or a write-enabled repository deploy key). The private key stays in GitLab.

This operational branch, `codex/github-release-mirror`, has a separate CI config
for scheduled/web release synchronization. Normal build CI and main are
unchanged. Do not merge this branch's `.gitlab-ci.yml` into main.

Create a GitHub fine-grained personal access token restricted to Encore with
Contents read/write. Save it as the masked, hidden GitLab project variable
`GITHUB_RELEASE_TOKEN`. This operational branch is not protected, so the
variable must be available to it. Project collaborator permissions govern
access to pipelines and variables. Rotate the token before it expires.
GitLab access uses the short-lived `CI_JOB_TOKEN`; no GitLab PAT is needed.

Schedule a pipeline on this branch hourly, or run one from GitLab after
publishing a release. It reads all published GitLab releases, verifies each
GitHub tag resolves to the same commit, and copies notes and all Package links
as actual GitHub release assets with SHA-256 verification. New releases stay
in draft until uploads finish. Retries resume drafts and skip verified uploads.
It never deletes releases or replaces assets; a changed asset with an existing
name requires manual resolution. Future-dated releases are skipped until due.

Add compiled binaries to GitLab releases as Package links and retain their CI
artifacts before expiry. Missing tags/assets fail the job; retry after the
native mirror catches up or the source link is repaired. Release deletions
and asset removals are intentionally not propagated.
