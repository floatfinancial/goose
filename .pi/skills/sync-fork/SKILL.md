---
name: sync-fork
description: Sync Float's goose fork with upstream aaif-goose/goose. Fetches upstream, creates a dated sync branch, merges upstream/main, resolves conflicts using the patch manifest at docs/float-fork/PATCHES.md, runs the full test gate, opens a PR. Triggers on "sync fork", "sync upstream", "/sync-fork", "pull from upstream", "merge upstream", or "rebase from upstream".
---

# sync-fork

Keeps `floatfinancial/goose` current with `aaif-goose/goose` (upstream) without
losing Float's patches. **Read `docs/float-fork/PATCHES.md` before doing
anything else** — it's the source of truth for what we've patched and how to
resolve conflicts on those files.

## Preflight (bail if any fail)

Run each in order. Stop and surface the failure to the user; do not attempt to
fix.

```bash
git status --porcelain           # must be empty
git rev-parse --abbrev-ref HEAD  # must be "main"
git fetch origin main --quiet
git status -sb                   # must show "up to date with 'origin/main'"
test -f docs/float-fork/PATCHES.md
git remote get-url upstream      # must resolve; otherwise `git remote add upstream …`
```

If `upstream` remote is missing:
```bash
git remote add upstream git@github.com:aaif-goose/goose.git
```

## Detect delta

```bash
git fetch upstream --quiet
COUNTS=$(git rev-list --left-right --count upstream/main...origin/main)
# format: "<upstream_only> <fork_only>"
```

- Upstream-only count == 0: nothing to sync. Report "already current" and exit.
- Upstream-only > 0: proceed.

Optionally list the new upstream commits to give the user a heads-up:
```bash
git log --oneline origin/main..upstream/main | head -50
```

## Sync branch

```bash
BRANCH="sync/upstream-$(date +%Y-%m-%d)"
git checkout -b "$BRANCH"
git merge upstream/main --no-ff --no-commit
```

If `git merge` reports "Already up to date." something went wrong in preflight
— stop and report.

## Conflict resolution

If the merge is clean (no `UU` / `DU` / `UD` entries in `git status`), skip to
the **Verification gate**.

Otherwise, for each conflicted file:

1. Look it up in `docs/float-fork/PATCHES.md`.

2. **If listed**: follow its `On merge conflict` recipe.

3. **If NOT listed**: this is a red flag — it means someone patched a new
   upstream file without updating the manifest. Do not auto-resolve. Report:
   > "`<path>` has a merge conflict but isn't in `PATCHES.md`. This means a
   > previous change touched an upstream file without documenting it. Stopping
   > for human review — please inspect the conflict, decide the resolution,
   > and add an entry to `PATCHES.md` before continuing."

4. **Special-case files (always follow these, they override the manifest):**
   - `Cargo.lock`: `git checkout --theirs Cargo.lock` then `cargo build` to re-resolve.
   - `pnpm-lock.yaml`: `git checkout --theirs pnpm-lock.yaml` then `pnpm install --frozen-lockfile=false` in each affected workspace.
   - Any delete/modify conflict (`git status` shows `DU` or `UD`): always keep the delete for files listed as `(deleted)` in the manifest.

5. **The 30-line rule.** If the conflict block on a single file is more than
   30 lines, do not try to hand-merge it. Instead:
   ```bash
   git checkout --theirs <path>
   ```
   Then re-apply our documented change from the manifest by hand against the
   fresh upstream version. This produces a smaller, more reviewable diff than
   picking through a huge 3-way conflict.

After resolving each file:
```bash
git add <path>
```

## Verification gate

**All of the following must pass.** If any fails, do not commit — surface the
failure. Never bypass with `--no-verify` or by disabling a test.

The Rust test invocations mirror upstream CI in `.github/workflows/ci.yml`
(matrix `tls-feature`). Running `cargo test -p goose` with default features is
wrong — upstream's crates gate their crypto backends (jsonwebtoken,
rustls/native-tls, aws-lc-rs) and their platform extensions (code-mode) behind
features, and default features are empty. Match upstream or you'll chase
ghosts.

```bash
source bin/activate-hermit

# Rust — mirror upstream CI's tls-feature matrix job.
cargo fmt --check
cargo build
cargo clippy --all-targets -- -D warnings
cargo test -p goose --no-default-features --features rustls-tls,code-mode
cargo test -p goose --features aws-providers --lib bedrock_float
cargo test -p goose-providers --no-default-features --features rustls-tls
cargo test -p goose-cli --no-default-features --features rustls-tls,code-mode -- --skip scenario_tests::scenarios::tests
cargo test -p goose-cli --no-default-features --features rustls-tls,code-mode --jobs 1 scenario_tests::scenarios::tests

# Desktop UI
cd ui/desktop && pnpm typecheck && pnpm test && cd -

# Smoke: our custom subcommand parses
cargo run -p goose-cli --bin goose --features aws-providers -- auth aws-sso --help >/dev/null
```

For a fuller sweep on a big sync, also run `just check-everything`.

## Commit and open PR

```bash
git commit --no-verify=false  # commit the merge
git push -u origin "$BRANCH"
```

PR body template:

```md
## Why
Sync Float's fork with `aaif-goose/goose` upstream. <N> upstream commits merged.

## What
- Merges `upstream/main` (<upstream sha>) into `main`.
- Conflicts resolved: <count> — see per-file notes below.
- New upstream commits: (link to `git log --oneline origin/main..upstream/main`)

### Conflicts
<For each conflicted file, one line: path — resolution taken (manifest recipe / --theirs + reapply / clean auto-merge).>

### Verification
- [x] cargo fmt --check
- [x] cargo clippy --all-targets -- -D warnings
- [x] cargo test -p goose (+ aws-providers feature)
- [x] cargo test -p goose-cli
- [x] pnpm typecheck / pnpm test in ui/desktop
- [x] `goose auth aws-sso --help` smoke

### Residual risk
<Anything that felt awkward: an unusual conflict shape, a test that changed
behaviour subtly, a new upstream feature that touches our patch surface.>
```

Create the PR:
```bash
gh pr create --base main --title "sync: upstream/main $(git rev-parse --short upstream/main)" --body "<body from template>"
```

## Merge policy

- **Zero conflicts + green gate**: safe to merge. Squash or merge-commit —
  Float's policy is merge-commit for sync PRs so upstream commit hashes stay
  greppable.
- **Any Tier-C conflict** (any file in the manifest with substantive changes,
  not just additive registrations): **require human review**. Do not
  auto-merge. Tag the last person who touched that file for eyes.
- **Any unlisted patched file** (per step 3 above): halted at conflict resolution.
  Do not proceed to PR until the manifest is updated in the same branch.

## After merge

```bash
git checkout main
git pull origin main
git branch -D "$BRANCH"       # local cleanup
git push origin --delete "$BRANCH"  # remote cleanup
```

Then check `PATCHES.md` — did the sync surface anything that should be added
or amended? If so, open a follow-up PR.

## Recovery

If the merge goes sideways mid-way through:

```bash
git merge --abort              # bail from the merge
git checkout main              # back to safe ground
git branch -D "$BRANCH"        # kill the sync branch
```

Then start over. No shame in a fresh attempt — sync-fork is idempotent.
