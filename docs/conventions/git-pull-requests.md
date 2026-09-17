# Pull requests

This explains how PRs are opened, titled, versioned, and — critically — who merges them. Branch creation and upkeep is covered in [[git-branching]]; commit message form in [[git-commits-and-versioning]].

## Never add a "prepared by" trailer to a PR description

Same reasoning as for commit co-authoring.

## Open the PR

- Base = the trunk you branched from ([[git-branching]]). Set it explicitly:
  `gh pr create --base <trunk>`.
- If the change touches a documented pattern/convention/architecture, update the relevant  `docs/` page in the same PR; non-trivial decisions get an ADR.

## PR title

The title follows conventional-commit form (`CODE: Capitalised summary`) using the
"greatest" commit code in the PR (top-most in the [[git-commits-and-versioning]] type-code table). It matters because it is used as the squash-merge commit: it becomes the GitHub release title via `release.yml`, and `update-pull-request.yml` groups commits by code in the auto-generated PR changelog.

## Never merge a pull request

Opening a PR is the assistant's job; merging it is the human's — in every repo, without exception, including small fixes, green CI, doc-only changes, and PRs the assistant authored itself. `gh pr merge`, the GitHub API, and the web UI all run as the signed-in user, so a merge commits *their* account to code they have not reviewed. Open the PR, report the URL, and stop there. The same applies to force-pushing over someone's branch or deleting branches you did not create.

## Version bumps

Bump `version` in `pyproject.toml` **only on PRs whose base is `main`** — never on sub-branch PRs into a trunk like `geo` (the `semantic` check only runs on `main`-bound PRs, and trunk-PR bumps just create `pyproject.toml` conflicts). How to calculate and apply the bump is in [[git-commits-and-versioning]].

## Related notes

- [[git-branching]] — bases, naming, protected branches, updating
- [[git-commits-and-versioning]] — type codes, breaking changes, semver calculation
