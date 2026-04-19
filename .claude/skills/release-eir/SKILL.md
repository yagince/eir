---
name: release-eir
description: Cut a new release for the eir GitHub PR/Issue watcher. Use this skill whenever the user asks to release, cut a release, bump the version, tag a release, or publish a new build — including short phrases like "release v0.2.0", "bump to 0.3", "タグ打ってリリース", "新しいリリース出して". The skill bumps the version in all three manifest files, runs a pre-flight check, commits, tags `v<version>`, and pushes. The release workflow in `.github/workflows/release.yml` then builds the macOS bundle for both arm64 and x64 and attaches them to a draft GitHub Release.
---

# Releasing eir

Every release goes through the same short checklist: verify the tree is clean, bump the three version files, commit, tag, push. The release workflow (`.github/workflows/release.yml`) picks up the tag and builds artefacts — your job is to set up that tag correctly so the workflow has good inputs.

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

## Verify

After pushing, confirm the workflow kicked off and give the user links:

```bash
gh run list --workflow=release.yml --limit 1
gh run watch           # optional — follow the active run
```

Once the workflow finishes (typically 5–10 minutes per target; arm64 and x64 macOS run in parallel) the draft release lands at:

```bash
gh release view v<version> --web
```

Tell the user to review the draft release's artefacts and publish when ready. The release stays drafted until someone explicitly publishes, so a bad build can be deleted without affecting downstream consumers.

## Recovering from a bad release

Only do this when the user explicitly asks. Tag deletion is destructive and visible to anyone who has pulled.

Before pushing the tag:

```bash
git reset --hard origin/main
git tag -d v<version>
```

After the tag is already pushed but the release is broken:

```bash
gh release delete v<version>          # or delete via the web UI
git push origin :v<version>           # remove the remote tag
git tag -d v<version>                 # remove the local tag
```

Then revert or amend the version commit and redo the release with a fresh version number. Avoid reusing a published version number — downstream caches will think they already have that release.

## Why three files

- `Cargo.toml` (plus `Cargo.lock`) drives the Rust build and embeds the version in the compiled binary metadata.
- `package.json` is what the frontend tooling and `pnpm version` care about.
- `tauri.conf.json` is the one Tauri actually reads at build time to embed the version into the bundled `.app` and its `Info.plist`.

If these three drift, the About dialog, the `Info.plist`, and `cargo --version`-style queries end up disagreeing — subtly painful when debugging a specific user's build.
