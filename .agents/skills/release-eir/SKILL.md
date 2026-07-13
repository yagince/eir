---
name: release-eir
description: Cut a new release for the eir GitHub PR/Issue watcher end-to-end — bump the version, push the tag, wait for the release workflow to build all four platforms, attach auto-generated release notes, and publish the draft. Use this skill whenever the user asks to release, cut a release, bump the version, tag a release, or publish a new build — including short phrases like "release v0.2.0", "bump to 0.3", "タグ打ってリリース", "新しいリリース出して". The skill bumps the version in all three manifest files, runs a pre-flight check, commits, tags `v<version>`, pushes, watches `.github/workflows/release.yml` until the arm64 / x64 / linux / windows matrix finishes, fills the draft release body with GitHub's auto-generated "What's Changed" notes, and flips the draft to published.
---

# Releasing eir

Every release goes through the same arc: verify the tree is clean, bump the three version files, commit, tag, push, wait for the workflow to build all four platforms, attach the auto-generated release notes, and publish. The release workflow (`.github/workflows/release.yml`) fans out into a 4-way matrix (macOS arm64/x64, linux-x64, windows-x64) and uploads each bundle into a single pre-created draft release. Your job is to drive that draft all the way to "published" — don't stop at the tag push.

## Preconditions

Verify all of these before making any changes. If any fail, stop and report to the user rather than trying to paper over the problem.

- Current branch is `main`, and `main` is up to date with `origin/main`:
  ```bash
  git fetch origin
  git switch main
  git pull --ff-only
  ```
- Working tree is clean:
  ```bash
  git status --porcelain  # must be empty
  ```
- All tests pass:
  ```bash
  cd src-tauri && cargo test --lib && cd ..
  pnpm test
  ```

Do not "fix" these by stashing uncommitted work, force-pulling, or skipping tests. Those shortcuts hide real problems that show up in the release.

## Pick the version

If the user already named a version, just use it. Otherwise investigate the diff since the last release, suggest a semver bump, and confirm the pick with **AskUserQuestion** before touching any files.

### 1. Find the last release

```bash
git describe --tags --abbrev=0 --match 'v*' 2>/dev/null
```

If that fails (no prior tag), treat the baseline as the first commit — the first release is typically `v0.1.0`. In that case skip the diff analysis and jump straight to AskUserQuestion with `v0.1.0` as the suggestion.

### 2. Read the commits since that tag

```bash
git log <last_tag>..HEAD --oneline --no-merges
```

Classify each commit subject by its conventional-commit prefix (or by its content if no prefix):

- `feat:` / `feat(...):` / subject starting with `add`, `introduce`, `support` → **new feature** → bumps minor
- `fix:` / `fix(...):` / subject starting with `fix`, `correct`, `resolve` → **bug fix** → bumps patch
- Subject contains `!` before the colon (e.g., `feat!:` or `refactor!:`), or the body contains `BREAKING CHANGE:` → **breaking change** → bumps major
- `chore:`, `docs:`, `test:`, `refactor:`, `ci:`, `style:` → usually no bump on their own; combine with the other categories to decide

### 3. Suggest a bump

Apply the rules:
- Any breaking change → **major**
- Else any feature → **minor**
- Else any fix (or non-trivial chore / refactor worth shipping) → **patch**
- Else nothing worth releasing → tell the user and stop

Compute the candidate version from the current version and the bump level. For `0.x.y` projects (pre-1.0), prefer **minor** for features and **patch** for everything else; `major` only when the user explicitly says so, since minor is the conventional "breaking change channel" under 1.0.

### 4. Confirm via AskUserQuestion

Present the findings and the suggested bump as a question. Summarise what shipped since the last tag in one or two lines above the options, so the user can evaluate without having to re-read the git log.

Example shape:

```
"N commits since v0.1.0 — X features, Y fixes, Z chores (no breaking changes).
Which version should the next release be?"
```

With options like:

- `patch — v0.1.1` (bug fixes only)
- `minor — v0.2.0` (recommended: includes new features)
- `major — v1.0.0` (breaking change / 1.0 graduation)
- `custom` (user will type an exact version)

Mark the computed recommendation clearly (e.g., prefix with `recommended:`) so the user can pick it without thinking.

### 5. Normalise the chosen version

Whatever the user picks, normalise to two forms for later steps: bare (`0.2.0`, for the manifest files) and tag (`v0.2.0`, for git).

## Bump the version

The version lives in three files and they must stay in sync:

1. `src-tauri/Cargo.toml` — `[package].version = "..."`
2. `package.json` — top-level `"version"`
3. `src-tauri/tauri.conf.json` — top-level `"version"`

After editing, refresh `Cargo.lock` so it lands in the same commit and the tree ends up clean:

```bash
cd src-tauri && cargo check && cd ..
```

## Commit, tag, push

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock \
        package.json src-tauri/tauri.conf.json
git commit -m "chore: release v<version>"
git tag v<version>

git push origin main
git push origin v<version>
```

Use a plain `chore: release vX.Y.Z` subject with no body — it keeps `git log --oneline` scannable. Both pushes are required; the release workflow only triggers on the tag push.

## Wait for the release build

The skill is not done when the tag is pushed — the draft release is created almost immediately by the `create-release` job, but it's an empty shell until the four matrix builds upload their bundles. You must wait and confirm success before touching the release body or flipping it out of draft. A "release" with only two of four platforms uploaded is worse than no release at all, because users silently pick up an incomplete set.

### 1. Grab the run id for the tag push

`gh run list` can race the tag push by a second or two (Actions needs time to register the ref), so poll a few times rather than assuming the first call returns the right row. Match on `headBranch` because for a tag push GitHub exposes the tag name there — this avoids accidentally grabbing an older run of the same workflow:

```bash
TAG=v<version>
for i in 1 2 3 4 5 6; do
  RUN_ID=$(gh run list --workflow=release.yml --limit 5 \
    --json databaseId,headBranch,event \
    --jq "map(select(.event==\"push\" and .headBranch==\"$TAG\"))[0].databaseId")
  [ -n "$RUN_ID" ] && [ "$RUN_ID" != "null" ] && break
  sleep 5
done
echo "Watching run $RUN_ID"
```

### 2. Watch until the matrix is done

```bash
gh run watch "$RUN_ID" --exit-status --compact
```

`--exit-status` is what makes this usable in a script: the command returns non-zero if any matrix job fails, so the next step can short-circuit. Typical wall time is 8–15 minutes — the arm64/x64 macOS jobs run in parallel with linux and windows, and the slowest one gates everything.

If `gh run watch` exits non-zero, **do not proceed to publish**. Dump the failure and hand control back to the user:

```bash
gh run view "$RUN_ID" --log-failed | tail -200
gh release view "$TAG" --web   # so they can inspect the half-filled draft
```

At that point the right move is usually to delete the draft + tag (see "Recovering from a bad release"), fix the root cause on `main`, and rerun the whole skill with the same version number — the release was never visible, so reusing the number is fine.

## Attach generated release notes

GitHub's UI has a "Generate release notes" button that produces a `## What's Changed` block (list of merged PRs grouped by label, plus a contributors line and a compare link). The REST API exposes the same generator at `POST /repos/{owner}/{repo}/releases/generate-notes`, which is what we want here — it guarantees byte-for-byte parity with the UI button and doesn't require us to roll our own `git log` formatter.

Generate against the just-pushed tag, using the prior `v*` tag as the comparison base so the notes only cover this release's window:

```bash
REPO=$(gh repo view --json nameWithOwner --jq .nameWithOwner)
# Second entry in a reverse-semver-sorted tag list = the previous release.
# (First entry is the tag we just pushed.)
PREV_TAG=$(git tag --list 'v*' --sort=-version:refname | sed -n '2p')

NOTES_FILE=$(mktemp -t eir-release-notes.XXXXXX.md)
gh api "repos/$REPO/releases/generate-notes" \
  -f "tag_name=$TAG" \
  ${PREV_TAG:+-f "previous_tag_name=$PREV_TAG"} \
  --jq .body > "$NOTES_FILE"
```

If this is the first-ever release (no previous `v*` tag), the `previous_tag_name` flag is simply omitted and GitHub falls back to "all commits since the repo was created" — the notes end up a bit noisy but still correct.

Apply the notes to the draft:

```bash
gh release edit "$TAG" --notes-file "$NOTES_FILE"
```

Show the user the first ~30 lines of `$NOTES_FILE` so they can sanity-check the generated text before publish. A quick `head -30` is enough — the skill should surface content, not silently ship it.

## Publish the release

Flip the draft to published and explicitly mark it "Latest". `--latest` matters because GitHub's auto-detection sometimes keeps the previous release marked latest when a new one is marked prerelease-adjacent or when tag sort order is ambiguous; being explicit avoids the update channel (the in-app "Check for updates" flow reads `latest.json` from the release tagged "Latest") silently stalling on an older build.

```bash
gh release edit "$TAG" --draft=false --latest
gh release view "$TAG" --web
```

Report the release URL to the user in your final message. Do not also tell them "please publish when ready" — publish already happened.

## Recovering from a bad release

Only do this when the user explicitly asks, or when `gh run watch --exit-status` in the "Wait for the release build" step returned non-zero and you need to roll back before retrying. Tag deletion is destructive and visible to anyone who has already fetched.

Before pushing the tag (tag is only local):

```bash
git reset --hard origin/main
git tag -d v<version>
```

After the tag is pushed but the release has not been published yet (still a draft) — this is the common case when the workflow failed mid-matrix:

```bash
gh release delete v<version> --cleanup-tag --yes
```

`--cleanup-tag` deletes both the draft release and the remote tag in one call; then drop the local tag with `git tag -d v<version>`. You can safely **reuse the same version number** in the next attempt because the release was never published — no downstream cache has seen it.

If the release was already published (draft → public) and then found broken:

```bash
gh release delete v<version> --cleanup-tag --yes
git tag -d v<version>
```

But in this case do **not** reuse the version number. Bump to the next patch and redo — downstream updaters and package caches may have already fetched `latest.json` for the broken version and will refuse to re-download the same tag.

## Why three files

- `Cargo.toml` (plus `Cargo.lock`) drives the Rust build and embeds the version in the compiled binary metadata.
- `package.json` is what the frontend tooling and `pnpm version` care about.
- `tauri.conf.json` is the one Tauri actually reads at build time to embed the version into the bundled `.app` and its `Info.plist`.

If these three drift, the About dialog, the `Info.plist`, and `cargo --version`-style queries end up disagreeing — subtly painful when debugging a specific user's build.
