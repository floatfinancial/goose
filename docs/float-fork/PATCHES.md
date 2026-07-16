# Float fork patch manifest

Every upstream file we've modified, why, and how to resolve a merge conflict on
it during `sync-fork`. The `sync-fork` skill reads this file first.

Two categories:

- **Net-new files** — owned by us, cannot conflict. Listed for provenance only.
- **Patched upstream files** — where merge conflicts happen. Each entry has an
  explicit "on merge conflict" recipe.

Rule of thumb: if a conflict on a patched file is over 30 lines, don't try to
resolve it. `git checkout --theirs <file>`, then re-apply the change from
this manifest by hand. That's cheaper and less error-prone.

---

## Net-new files (ours)

| Path | Purpose |
|---|---|
| `crates/goose/src/providers/aws_sso.rs` | AWS SSO / Identity Center device-code flow. Called by `goose auth aws-sso`. |
| `crates/goose/src/providers/bedrock_float.rs` | Float-owned Bedrock helpers: runtime model enumeration via `ListInferenceProfiles` / `ListFoundationModels`, plus the "pin sonnet-5 / opus-4-8 to top" logic. |
| `crates/goose-cli/src/commands/auth.rs` | Impl of `goose auth aws-sso` subcommand. |
| `ui/desktop/src/awsSsoIpc.ts` | Electron main-side IPC bridge for the AWS SSO sign-in modal. |
| `ui/desktop/src/components/onboarding/AwsSsoModal.tsx` | Sign-in modal UI. |
| `ui/desktop/src/distroConfig.ts` | Distro-wide constants: default SSO URLs/regions, `VISIBLE_PROVIDERS` for the tile grid. **The one file every downstream distro should edit.** |
| `ui/desktop/src/acp/providersFloat.ts` | Float-owned wrappers for provider listing: visible-provider filter + runtime-model enumeration. |

---

## Patched upstream files

### `crates/goose/src/providers/bedrock.rs`

**What we changed:**
- `BEDROCK_DEFAULT_MODEL` swapped to `"global.anthropic.claude-sonnet-5"`.
- `BEDROCK_KNOWN_MODELS` gains two entries at the top: `"global.anthropic.claude-sonnet-5"`, `"us.anthropic.claude-opus-4-8"`.
- `BedrockProvider` struct gains one field: `sdk_config: Option<aws_config::SdkConfig>`.
- `BedrockProvider::from_env` initialises it: `sdk_config: Some(sdk_config)`.
- Two test-only struct literals init it: `sdk_config: None`.
- `fetch_supported_models` body: delegate to `bedrock_float::fetch_supported_models_via_aws`, fall back to `BEDROCK_KNOWN_MODELS` if empty or error.

**On merge conflict:**
- Constants block: keep our two extra IDs at the top; upstream may append more, keep those too.
- Struct field / init lines: keep our additions verbatim; they're additive.
- `fetch_supported_models` body: keep our delegation to `bedrock_float`. If upstream reshapes the fn signature (e.g. adds a param), adapt the delegation call.
- **If the conflict is unclear or spans >30 lines:** `git checkout --theirs crates/goose/src/providers/bedrock.rs`, then reapply each bullet above by hand.

### `crates/goose/src/providers/mod.rs`

**What we changed:**
- Added `pub mod aws_sso;` (unconditional).
- Added `#[cfg(feature = "aws-providers")] pub mod bedrock_float;` next to the existing `bedrock` mod.

**On merge conflict:** keep both `pub mod` lines. Upstream never touches them.

### `crates/goose/src/posthog.rs`

**What we changed:**
- `is_telemetry_enabled() -> bool` returns `false` unconditionally. Float distro sends no analytics.

**On merge conflict:** always keep the `false` return. Discard any upstream logic that consults env vars, config files, or user prompts. **This is a distribution policy, not a preference.**

### `crates/goose/src/acp/server/config.rs`

**What we changed:**
- Added `|| provider_id == "aws_bedrock"` to the model-existence check so opaque Bedrock model IDs pass through to the SDK instead of tripping our whitelist. Comment on why.

**On merge conflict:** keep the extra `||` branch and its comment. If upstream restructures the check, port the branch into the new shape.

### `crates/goose-cli/src/cli.rs`

**What we changed:**
- Added an `AuthCommand` enum + `handle_auth_subcommand` async fn (both after `GatewayCommand`).
- Added `Auth(AuthCommand)` variant to `Command`.
- Added the dispatch arm `Some(Command::Auth(cmd)) => handle_auth_subcommand(cmd).await`.
- Added `Some(Command::Auth(_)) => "auth"` to `get_command_name`.
- All five additions are gated behind `#[cfg(feature = "aws-providers")]` because the subcommand only makes sense when Bedrock is compiled in — without the gate, `cargo test -p goose-cli --no-default-features --features rustls-tls,code-mode` (upstream CI's TLS-matrix invocation) fails to compile.

**On merge conflict:** all four additions are independent inserts. Reapply each in the new file location. Keep the `#[cfg(feature = "aws-providers")]` attribute on all of them.

### `crates/goose-cli/src/commands/mod.rs`

**What we changed:** added `#[cfg(feature = "aws-providers")] pub mod auth;` at the top. The `cfg` gate is required so `cargo test -p goose-cli --no-default-features` (upstream CI shape) still builds — without it, `auth.rs` fails to compile because it uses `goose::providers::aws_sso` and `goose::providers::bedrock`, both gated behind `aws-providers` in the `goose` crate.

**On merge conflict:** keep the line and its `cfg` gate.

### `crates/goose/Cargo.toml`

**What we changed:** added AWS SSO / STS / SDK deps + feature list entries for the `aws-providers` feature. See git log for the exact set. Also added `"serde"` to `indexmap` features — upstream removed `utoipa` in #10505, which had been transitively enabling `indexmap/serde`; without it, `crates/goose/src/config/providers.rs` fails to build when checking `-p goose` alone (feature unification saves the workspace build, but not per-crate checks).

### `crates/goose/src/agents/reply_parts.rs` (test-only)

**What we changed:** the `prepare_tools_returns_sorted_tools_including_frontend` test now constructs the `Agent` via `Agent::with_config` + a fresh `SessionManager::new(tempdir)` instead of `Agent::new()`. `Agent::new()` routes through the global `SESSION_STORAGE` `LazyLock` in `crates/goose/src/session/session_manager.rs:56`, which captures `Paths::data_dir()` at first-access. Other tests in the crate (`goose_apps::cache::tests::with_temp_config`, `hints::load_hints::tests::test_global_agents_md_*`) mutate `GOOSE_PATH_ROOT` via raw `env::set_var` (no `env_lock`), so the singleton can lock onto a `TempDir` that later drops, and any subsequent `create_session` call fails with `SQLITE_CANTOPEN`. This flaked intermittently (~80% failure rate under our default `cargo test -p goose --no-default-features --features rustls-tls,code-mode` invocation on Apple Silicon). The fix is a hermetic Agent per test; the underlying architecture bug is upstream's.

**Upstream status:** to be filed against `aaif-goose/goose` — proposed fixes are (a) refactor the two `LazyLock` singletons in `session_manager.rs` and `permission.rs` so path resolution isn't captured at first-access, or (b) audit every `env::set_var("GOOSE_PATH_ROOT", …)` in the test tree and require `env_lock::lock_env`. When either lands upstream and reaches our fork, retire this patch.

**On merge conflict:** if upstream modifies the same test, replay the hermetic pattern (`Agent::with_config` + `SessionManager::new(tempdir)`) into the new shape. If upstream fixes the singleton bug directly, drop this patch entirely.

**On merge conflict:** keep our added lines. If upstream renames the feature or restructures the deps table, port our additions into the new shape. **Never hand-edit `Cargo.lock` — run `cargo build` to re-resolve.**

### `Cargo.lock`

**On merge conflict:** always `git checkout --theirs Cargo.lock`, then `cargo build` to re-resolve. Never manually edit.

### `ui/desktop/src/App.tsx`

**What we changed:** removed the `TelemetryConsentPrompt` import and its `<TelemetryConsentPrompt />` render.

**On merge conflict:** keep both deletions. If upstream also deletes the file (unlikely), no conflict. If upstream expands the component, discard the expansion — Float distro has no telemetry consent because posthog is disabled at source.

### `ui/desktop/src/components/TelemetryConsentPrompt.tsx` **(deleted)**

**On merge conflict (delete/modify):** always keep the delete. `git rm ui/desktop/src/components/TelemetryConsentPrompt.tsx`.

### `ui/desktop/src/main.ts`

**What we changed:**
- Added `import { registerAwsSsoIpc } from './awsSsoIpc';` near the other imports.
- Added a `registerAwsSsoIpc({ isPackaged, resourcesPath })` call inside `appMain` next to other IPC registrations.

**On merge conflict:** re-insert both lines against upstream's new layout.

### `ui/desktop/src/preload.ts`

**What we changed:**
- Added three fields to the `ElectronAPI` type: `awsSsoStart`, `awsSsoGetDefaults`, `onAwsSsoEvent`.
- Added the matching implementations in the `electronAPI` object.

**On merge conflict:** re-insert the six chunks. All additive, no upstream touches.

### `ui/desktop/src/acp/providers.ts`

**What we changed (small patch):**
- One import line: `import { filterVisibleProviders, enhanceProviderModels } from './providersFloat';`.
- `acpListProviderDetails` calls `filterVisibleProviders(entries).map(...)` instead of `entries.map(...)`.
- `acpListProviderModels` returns `enhanceProviderModels(client, providerId, staticModels)` instead of the raw `staticModels`.

**On merge conflict:** re-insert the three edits. All heavy lifting lives in `providersFloat.ts` and doesn't conflict.

### `ui/desktop/src/components/onboarding/ProviderConfigForm.tsx`

**What we changed:**
- Added `import AwsSsoModal from './AwsSsoModal';`.
- Added `const [showAwsSsoModal, setShowAwsSsoModal] = useState(false);`.
- Added `const isBedrock = provider.name === 'aws_bedrock';`.
- Added an `if (isBedrock) return <BedrockSignInForm />;` branch at the top of `renderForm`.
- Added `<AwsSsoModal open={...} onOpenChange={...} />` at the end of the returned JSX.

**On merge conflict:** re-insert each additive chunk. If upstream introduces its own AWS Bedrock onboarding UI, stop and decide whether to keep ours, keep theirs, or merge — do not auto-resolve.

### `AGENTS.md`

**What we changed:** rewrote our development instructions.

**On merge conflict:** keep ours entirely — this is Float's fork guidance, not upstream's.

---

## Branding rebrand: Goose → Sponge

Every user-visible "Goose" string in the desktop app was renamed to "Sponge" and every icon replaced with the Twemoji sponge glyph (U+1F9FD, MIT/CC-BY-4.0). Tier 1 rebrand only — CLI binary is still `goose`, Rust crate names, env vars, config dir, and the `goose://` deeplink scheme are unchanged. See `CUSTOM_DISTROS.md` section D for scope. Tier 2 items are deferred; the debt is tracked in `sponge:` comments.

### `ui/desktop/package.json`

**What we changed:** `productName` → `"Sponge"`, `description` → `"Sponge App"`. `name` (`"goose-app"`) is npm-internal, left alone.

**On merge conflict:** keep our two string values. Version bumps from upstream flow through unchanged.

### `ui/desktop/index.html`

**What we changed:** `<title>Goose</title>` → `<title>Sponge</title>`.

**On merge conflict:** keep ours.

### `ui/desktop/src/main.ts` (branding additions on top of existing SSO patch)

**What we changed (branding):**
- About-panel `applicationName: 'Sponge'`.
- Main-window `title: 'Sponge'`.
- macOS app-menu label match `item.label === 'Sponge'`.

**On merge conflict:** three independent string edits, reapply verbatim. Same file also holds the AWS SSO IPC additions listed earlier — resolve both patches together.

### `ui/desktop/src/utils/winShims.ts`

**What we changed:** literal `'Sponge'` in the Windows shim (was `'Goose'`).

**On merge conflict:** keep the rename.

### `ui/desktop/src/utils/autoUpdater.ts`

**What we changed:** `trayRef.setToolTip('Sponge')` (was `'Goose'`).

**On merge conflict:** keep the rename.

### `ui/desktop/src/utils/githubUpdater.ts`

**What we changed:** default value of `bundleName` is `'Sponge'` (was `'Goose'`). Env var name `GOOSE_BUNDLE_NAME` is kept — dev-facing, tier 2.

**On merge conflict:** keep the default rename. If upstream renames the env var, port over the new name; do not rename it ourselves.

### `ui/desktop/src/components/sessions/SessionViewComponents.tsx`

**What we changed:** assistant role `defaultMessage: 'Sponge'` (was `'Goose'`). The message id `sessionViewComponents.role.assistant` is unchanged.

**On merge conflict:** keep the rename; the id is stable.

### `ui/desktop/src/components/icons/Goose.tsx`

**What we changed:** replaced the hand-drawn goose `<path>` inside `Goose()` with the Twemoji sponge (U+1F9FD, MIT/CC-BY-4.0) paths. `viewBox` changed from `0 0 24 24` to `0 0 36 36`. The component signature, exported name (`Goose`), and props (`className`) are unchanged so every callsite (`OnboardingGuard`, `GooseLogo`, `LoadingGoose` → `GooseLogo`) picks up the sponge automatically. `Rain()` (the wind animation) is untouched.

**On merge conflict:** always keep our body; upstream may re-illustrate the goose, discard those changes. If upstream renames/removes the file, restore the sponge component under the same path.

### `ui/desktop/src/components/onboarding/OnboardingGuard.tsx`

**What we changed:** two `defaultMessage` strings — `welcomeTitle` → `"Welcome to Sponge"`, `checkProviderErrorTitle` → `"Unable to connect to Sponge server"`. Message ids unchanged.

**On merge conflict:** keep our two `defaultMessage` values. Do not import `Goose` from anywhere else — the current import path (`../icons`) points at the sponge component.

### `ui/desktop/src/components/BaseChat.tsx`

**What we changed:** the top-right "goose" watermark inside the chat pane. Replaced the `<a href="https://goose-docs.ai">` wrapper with a plain `<div>` (no external link, no `docs.ai` reference), and changed the visible label text from `goose` to `sponge`. The `<Goose />` icon component call is unchanged — it now renders the sponge SVG anyway. Comment updated for evergreen naming.

**On merge conflict:** keep our block. If upstream changes the watermark to a different asset or destination, prefer ours — the link removal is intentional (Sponge doesn't ship with a public docs site).

### `ui/desktop/src/components/LoadingGoose.tsx`

**What we changed:** five `defaultMessage` strings (`thinking`, `streaming`, `waiting`, `compacting`, `idle`) now read `"Sponge is …"` (was `"goose is …"`). Message ids and component structure unchanged. The filename and component name (`LoadingGoose`) are intentionally kept — tier 2. `STATE_ICONS[ChatState.Streaming]` was repointed from `<FlyingBird>` to `<AnimatedIcons cycleInterval={600}>` (same component the Thinking/Compacting states use), and the `FlyingBird` import was removed. The `FlyingBird` component itself is untouched — still consumed by `McpAppRenderer.tsx`.

**On merge conflict:** keep our five string values; the id namespace `loadingGoose.*` is stable. Keep the AnimatedIcons repoint — the flapping-bird animation was Goose-specific brand imagery.

### `ui/desktop/src/i18n/messages/*.json` and `ui/desktop/src/i18n/compiled/*.json`

**What we changed:** for every locale, substituted `Goose`/`goose` with `Sponge` inside the `defaultMessage` values of these keys only:
- `sessionViewComponents.role.assistant`
- `onboardingGuard.welcomeTitle`
- `onboardingGuard.checkProviderErrorTitle`
- `loadingGoose.thinking`, `loadingGoose.streaming`, `loadingGoose.waiting`, `loadingGoose.compacting`, `loadingGoose.idle`

All other `Goose` occurrences in locale files are left in place — tier 2 debt (see below).

**On merge conflict:** keep the rename for these keys. When upstream adds new locales, apply the same key list before finalizing the merge. When upstream adds new user-visible messages that mention Goose, decide case by case whether they belong on the tier-1 list.

### `ui/desktop/forge.config.ts`

**What we changed:**
- Protocol registration renamed to `SpongeProtocol` (scheme stays `goose` — tier 2).
- `NSCalendarsUsageDescription` / `NSRemindersUsageDescription` say "Sponge".
- deb maker: `name: 'Sponge'`, `bin: 'Sponge'`, `maintainer: 'Float'`, `homepage: 'https://floatfinancial.com/'`.
- rpm maker: same as deb.
- flatpak maker: `id: 'com.float.sponge'` (was `'io.github.block.Goose'` — a fresh Sponge distro has no legacy installs), `bin: 'Sponge'`, `homepage: 'https://floatfinancial.com/'`.

**On merge conflict:** the file has three distinct patches (Info.plist strings, three makers, protocol name). Reapply each block; if upstream restructures the makers list, port each rename into the new shape. **Do not** revert the flatpak id — Sponge is not backwards-compatible with the Block Goose installs.

### `ui/desktop/forge.deb.desktop`

**What we changed:** rewrote every line — `Name=Sponge`, `Exec=/usr/lib/sponge/Sponge %U`, `Icon=/usr/share/pixmaps/sponge.png`. `MimeType=x-scheme-handler/goose;` stays (protocol scheme is tier 2).

**On merge conflict:** keep ours entirely.

### `ui/desktop/forge.rpm.desktop`

**What we changed:** same as deb but capitalised paths (`/usr/lib/Sponge/Sponge`, `/usr/share/pixmaps/Sponge.png`) to match upstream's inconsistent conventions.

**On merge conflict:** keep ours entirely.

### `ui/desktop/src/images/` (icons replaced)

**What we changed:** every `icon.*`, `icon-*`, `glyph.svg`, `iconTemplate*.png` was replaced with the Twemoji sponge (U+1F9FD) rendered at the appropriate size and format. `loading-goose/` subdirectory (loading spinner assets) is untouched — only referenced by e2e test data-testids.

**On merge conflict:** always keep ours. If upstream adds new icon sizes or template variants, regenerate them from `icon.svg` using `prepare.sh`.

### `justfile`

**What we changed:** `just package-ui` output paths reference `Sponge-darwin-arm64/Sponge.app` (was `Goose-darwin-arm64/Goose.app`) because Electron Forge names the output by `productName`.

**On merge conflict:** reapply the two path renames in the `package-ui` recipe. Other `goose` references in the Justfile (CLI binary name, target paths) are intentionally kept.

### `ui/desktop/src/distroConfig.ts`

**What we changed (in addition to earlier SSO defaults):** added `APP_DISPLAY_NAME = 'Sponge'` at the top of the file. Not yet imported anywhere — future consumers should read from here rather than hardcoding.

**On merge conflict:** keep the constant. The file is Float-owned; conflicts should not occur.

---

## Deliberate rebrand debt (`sponge:` comments track these)

The rebrand did NOT touch these on purpose. Each has a lower value-to-cost ratio than the tier-1 changes. Upgrade only when the specific user-visible seam actually matters:

- CLI binary name: still `goose`. Dev audience, ~30 justfile / CI / scripts references. Upgrade when Sponge markets its CLI.
- Rust crate names: still `goose`, `goose-cli`, `goose-mcp`, etc. Zero user visibility, catastrophic sync-fork cost.
- Env var prefix: still `GOOSE_*` (`GOOSE_PROVIDER`, `GOOSE_MODEL`, `GOOSE_BUNDLE_NAME`, …). Dev-facing.
- Config directory: still `~/.config/goose/`. Only visible if the user goes looking.
- Deeplink scheme: still `goose://recipe?…`, `goose://extension/add?…`. ~15 code sites + tests. Upgrade when Sponge distributes deeplinks under its own brand.
- In-app body copy: hundreds of remaining `Goose` string occurrences in settings descriptions (`AppSettingsSection`, `KeyboardShortcutsSection`, `TelemetrySettings`, `ChatSettingsSection`, `ConfigSettings`, `AuthSettingsSection`, etc.), tooltips, empty states, blog posts, and docs. These are behind menus users open less often. Needs editorial rewrite, not find-replace — many read poorly if you swap the noun mechanically.
- System prompt in `crates/goose/src/prompts/system.md`: the assistant still self-identifies as "Goose". Bedrock model responses will say "I am Goose" until this is edited.

### `crates/goose/src/prompts/system.md`, `tiny_model_system.md`, `subagent_system.md`

**What we changed:** the first-line identity claim in each prompt now says "Sponge, distributed by Float" and describes Sponge as a downstream distribution of the upstream goose project by AAIF. Attribution to AAIF is preserved (ASL 2.0). Every other line of every prompt is untouched.

**On merge conflict:** keep our first-line rewrites. If upstream restructures the identity block into a variable or a Jinja block, port our text into the new shape. **A rebuild (`just release-binary`) is required for changes to take effect** — the prompts are `include_dir!`'d into the CLI binary at compile time.

---

## Auto-updater and GitHub distribution: aaif-goose → floatfinancial

Every hardcoded reference to the upstream repo (`aaif-goose/goose`) that would send auto-update checks, publisher pushes, or bug-report links to upstream instead of Float's fork is now `floatfinancial/goose`. The env-var override pattern (`GITHUB_OWNER`, `GITHUB_REPO`) is preserved everywhere it existed — only the fallback defaults changed.

### `ui/desktop/src/app-update.yml`

**What we changed:** rewrote to `owner: floatfinancial`. `repo: goose` and `updaterCacheDirName: goose-updater` are unchanged (repo name stays `goose`; the cache dir name is dev-facing tier-2 debt).

**On merge conflict:** always keep our version. This file is baked into every Mac release at `Contents/Resources/app-update.yml` and controls where electron-updater looks for new releases.

### `ui/desktop/forge.config.ts` (auto-updater publisher additions on top of the existing branding patch)

**What we changed (auto-updater):** publisher-github config default `owner` is `'floatfinancial'` (was `'aaif-goose'`). Env-var override with `GITHUB_OWNER` is preserved.

**On merge conflict:** keep the default rename. The env-var override and the `name` field (repo) stay untouched.

### `ui/desktop/vite.main.config.mts`

**What we changed:** the `process.env.GITHUB_OWNER` `define` default is `'floatfinancial'`. Same override pattern; only the fallback changed.

**On merge conflict:** keep the default rename.

### `ui/desktop/src/utils/githubUpdater.ts` (auto-updater additions on top of the existing branding patch)

**What we changed (auto-updater):** `owner` and `repo` defaults now read `'floatfinancial'` / `'goose'` (was `'aaif-goose'` / `'goose'`). Env-var override with `GITHUB_OWNER` / `GITHUB_REPO` preserved. Same file also holds the `bundleName` → `'Sponge'` default from the branding patch — resolve both together.

**On merge conflict:** keep the default rename.

### `ui/desktop/src/utils/autoUpdater.ts` (auto-updater additions on top of the existing branding patch)

**What we changed (auto-updater):** `feedConfig.owner` is the literal `'floatfinancial'` (was `'aaif-goose'`). This is a hardcoded literal (not env-var driven) because electron-updater's `setFeedURL` takes a static config. Same file also holds the tray-tooltip rename from the branding patch.

**On merge conflict:** keep the literal `'floatfinancial'`.

### `ui/desktop/scripts/verify-mac-update-resources.js`

**What we changed:** the assertion list now expects `owner: floatfinancial` in the baked `app-update.yml`.

**On merge conflict:** keep the rename; it must match `ui/desktop/src/app-update.yml` line for line or the CI verify step fails.

### `ui/desktop/src/components/ui/Diagnostics.tsx` and `ui/desktop/src/components/settings/app/AppSettingsSection.tsx`

**What we changed:** the "Report bug" / "Report issue" / "Feature request" URLs now point to `github.com/floatfinancial/goose/issues/new?...` (three URLs total). Sponge user bug reports should not flow to upstream aaif-goose.

**On merge conflict:** keep the rewrites. If upstream restructures the issue-report flow (e.g. moves to a form), port our owner rename into the new shape.

---

## Chopping upstream-only surfaces

Deleting files that only made sense inside the upstream aaif-goose org: `CODEOWNERS` pointing at a nonexistent team, workflows that hardcode the upstream Docker image (would fail on our fork), workflows guarded by `if: github.repository == 'aaif-goose/goose'` (would no-op on our fork forever), and issue/discussion templates linking to upstream's public docs site.

### `.github/CODEOWNERS` **(deleted)**

`@aaif-goose/goose-maintainers` is not a team in this org. Every PR was being blocked waiting for review from a nonexistent team.

**On merge conflict (delete/modify):** always keep the delete. `git rm .github/CODEOWNERS`. If Float wants ownership routing, add a new file with `@floatfinancial/<real-team>`.

### `.github/workflows/code-review.yml` **(deleted)**
### `.github/workflows/goose-issue-solver.yml` **(deleted)**
### `.github/workflows/goose-pr-reviewer.yml` **(deleted)**
### `.github/workflows/goose-release-notes.yml` **(deleted)**
### `.github/workflows/test-finder.yml` **(deleted)**

All five hardcoded `ghcr.io/aaif-goose/goose:latest` as their job container. That image isn't published to our fork's container registry, so every trigger would fail immediately.

**On merge conflict (delete/modify):** always keep the delete. If Float ever wants goose-driven PR review automation, either build and publish a `ghcr.io/floatfinancial/goose:latest` image or rewrite the workflows against an available runner image, then reintroduce the workflow under a Float name.

### `.github/workflows/cargo-deny.yml` **(deleted)**
### `.github/workflows/cargo-machete.yml` **(deleted)**
### `.github/workflows/dependabot-auto-merge.yml` **(deleted)**
### `.github/workflows/minor-release.yaml` **(deleted)**
### `.github/workflows/rebuild-skills-marketplace.yml` **(deleted)**
### `.github/workflows/scorecard.yml` **(deleted)**
### `.github/workflows/stale.yml` **(deleted)**
### `.github/workflows/update-hacktoberfest-leaderboard.yml` **(deleted)**
### `.github/workflows/update-health-dashboard.yml` **(deleted)**

All nine guarded with `if: github.repository == 'aaif-goose/goose'`. On our fork every job was a no-op that still consumed the actions runner spin-up. They existed to update upstream's public dashboards, hacktoberfest leaderboard, OSSF scorecard, dependabot auto-merge policy, and minor-release cadence. None apply to Float.

**On merge conflict (delete/modify):** always keep the delete. If Float needs a specific one (e.g. `cargo-deny` scanning), reintroduce a new workflow authored against Float's org, don't restore upstream's.

### `.github/workflows/docs-update-cli-ref.yml` **(deleted)**

Automated PR generator that updates the public docusaurus documentation site at `goose-docs.ai`. Float doesn't ship a public docs site.

**On merge conflict (delete/modify):** always keep the delete.

### `.github/ISSUE_TEMPLATE/bug_report.md`

**What we changed:** removed the two `https://goose-docs.ai/...` links (troubleshooting hub + diagnostics guide) from the intro block. Replaced with a single in-app pointer: "Settings → Diagnostics exports a diagnostics zip." Rest of the template intact.

**On merge conflict:** keep our intro block. If upstream changes the diagnostics capture flow, port only the underlying instruction (not the URL).

### `.github/DISCUSSION_TEMPLATE/qa.yml`

**What we changed:** removed the `goose-docs.ai` diagnostics link; changed the version-field label from "Goose version and environment" to "Sponge version and environment". Rest of the schema intact.

**On merge conflict:** keep the rename and the link removal.

### `README.md`

**What we changed:** replaced upstream's `goose` marketing README (badges, trendshift, discord, LF insights, repology packaging status) with a short Sponge-focused readme covering: what Sponge is, install paths for the two audiences (MDM + CLI download), what's different from upstream, working-on-the-fork commands, bug reporting, and license attribution to AAIF.

**On merge conflict:** always keep ours. Upstream's README is marketing for the public goose project and none of it applies to a private Float distribution.

### `MAINTAINERS.md` **(deleted)**
### `GOVERNANCE.md` **(deleted)**
### `CONTRIBUTING.md` **(deleted)**
### `CONTRIBUTING_RECIPES.md` **(deleted)**
### `RELEASE.md` **(deleted)**
### `RELEASE_CHECKLIST.md` **(deleted)**

Upstream governance docs describing the AAIF project's community review process, release cadence, and contributor onboarding. None apply to Float's private fork.

**On merge conflict (delete/modify):** always keep the delete. Float's internal release process lives in whatever internal ops doc / rollout guide we ship separately.

### `CLAUDE.md` **(deleted)**
### `.github/copilot-instructions.md` **(deleted)**

AI-agent instructions written for upstream contributors. Float uses `AGENTS.md`, which is our own.

**On merge conflict (delete/modify):** always keep the delete.

---

## Mac-only distribution (drop Windows + Linux)

Sponge ships to macOS only. Everything Windows- or Linux-shaped in the fork was either deleted or rewritten. `sync-fork` should not resurrect any of this from upstream.

### `.github/workflows/bundle-desktop-windows.yml` **(deleted)**
### `.github/workflows/bundle-desktop-linux.yml` **(deleted)**
### `.github/workflows/pr-comment-bundle-windows.yml` **(deleted)**
### `.github/workflows/publish-npm.yml` **(deleted)**

Windows and Linux desktop bundling; publishing SDK / binary packages to npm under `@aaif/*`. Sponge ships neither.

**On merge conflict (delete/modify):** always keep the delete.

### `.github/workflows/release.yml`

**What we changed:** removed the `bundle-desktop-linux`, `bundle-desktop-windows`, and `bundle-desktop-windows-cuda` jobs plus their entries in the `release` job's `needs:` list. Removed `*.deb`, `*.rpm`, `*.flatpak` artifact patterns and renamed `Goose*.zip` → `Sponge*.zip` in both `attest-build-provenance` and both `release-action` steps. Updated the `id-token` permissions comment to drop the "Windows signing" reference. `bundle-desktop` (macOS arm64) and `bundle-desktop-intel` are unchanged.

**On merge conflict:** keep the reduced job list and artifact patterns. If upstream renames the mac zip pattern (Goose/Sponge/etc.), port the rename.

### `.github/workflows/canary.yml`

**What we changed:** same shape as `release.yml` — removed Linux and Windows job entries, updated `needs:`, dropped `.deb`/`.rpm`/`.flatpak` artifact patterns, renamed `Goose*.zip` → `Sponge*.zip`, removed the `actions: read` permission (only bundle-desktop-windows needed it).

**On merge conflict:** same as release.yml — keep the reductions.

### `.github/workflows/bundle-desktop.yml`, `bundle-desktop-intel.yml`, `bundle-desktop-manual.yml`

**What we changed:** every hardcoded `Goose-darwin-arm64`/`Goose-darwin-x64` path renamed to `Sponge-*`; `Goose.app` → `Sponge.app`; `Goose.zip` → `Sponge.zip`; `Goose_intel_mac.zip` → `Sponge_intel_mac.zip`; `Contents/MacOS/Goose` (the actual executable inside the .app bundle, named by `productName`) → `Contents/MacOS/Sponge`. This tracks the `productName: "Sponge"` change in `ui/desktop/package.json` — Electron Forge names outputs from that value.

**On merge conflict:** keep the Sponge names. If upstream introduces new Goose-named paths in the same files (renaming outputs, adding new artifacts), port the Sponge rename into them.

### `.github/workflows/pr-comment-bundle.yml`, `pr-comment-bundle-intel.yml`, `release-branches.yml`

**What we changed:** same rename — `Goose-*`, `Goose.app`, `Goose.zip` → `Sponge-*`, `Sponge.app`, `Sponge.zip` in the artifact download links and quarantine-stripping instructions.

**On merge conflict:** keep the Sponge names.

### `ui/desktop/forge.config.ts`

**What we changed (Mac-only additions on top of earlier patches):** removed the `win32:` block (Windows signing config), the `maker-deb`, `maker-rpm`, `maker-flatpak` entries, the `isLinuxVulkanBuild` constant, and the `platforms: ['darwin', 'win32', 'linux']` + `options.icon: 'src/images/icon.ico'` fields on `maker-zip`. `maker-zip` now targets `['darwin']` only.

**On merge conflict:** keep the Mac-only shape. Discard any upstream Windows or Linux maker additions.

### `ui/desktop/package.json`

**What we changed (Mac-only additions):** removed `@electron-forge/maker-deb`, `-flatpak`, `-rpm`, `-squirrel` from `devDependencies` (kept `maker-zip`). Removed `node scripts/prepare-platform-binaries.js && ` prefix from `bundle:default` and `bundle:intel` scripts. Removed the stale `echo 'run --remote-debugging-port=8315' && ` prefix on `debug`. Updated `${GOOSE_BUNDLE_NAME:-Goose}` default → `${GOOSE_BUNDLE_NAME:-Sponge}` in `bundle:default`, `bundle:intel`, and `debug` (belt-and-suspenders — env var wins, default now matches product name).

**On merge conflict:** keep the dependency and script trims. **`ui/pnpm-lock.yaml` will need to be regenerated with `pnpm install` after the merge** — do not hand-edit the lockfile.

### `ui/desktop/scripts/prepare-platform-binaries.js` **(deleted)**
### `ui/desktop/src/platform/` **(deleted)**

Windows-specific: fetches `uv.exe`/`uvx.exe` from Astral's releases, stages them under `src/platform/windows/bin/`, and copies them into `src/bin/` at package time. Zero purpose on a Mac-only distribution.

**On merge conflict (delete/modify):** always keep the delete.

### `ui/desktop/src/utils/winShims.ts` **(deleted)**

Windows PATH-prep for shims (`uv.exe`, `uvx.exe`, `npx.cmd`). Early-returned on non-Windows platforms so it was a no-op on Mac anyway. Import in `main.ts` and the `ensureWinShims()` call were also removed.

**On merge conflict (delete/modify):** always keep the delete. Also drop any upstream re-import of `ensureWinShims` from `main.ts`.

### `ui/desktop/forge.deb.desktop` **(deleted)**
### `ui/desktop/forge.rpm.desktop` **(deleted)**

Linux desktop entry templates for the deb/rpm makers we removed.

**On merge conflict (delete/modify):** always keep the delete.

### `ui/desktop/scripts/generate-mac-update-manifest.js`

**What we changed:** the two hardcoded source/update filenames renamed from `Goose*.zip` to `Sponge*.zip`. This script generates the `latest-mac.yml` electron-updater manifest during release.

**On merge conflict:** keep the Sponge names — must match the outputs of `bundle:default` / `bundle:intel`.

### `justfile`

**What we changed (Mac-only additions on top of earlier patches):** removed the `release-windows`, `copy-binary-windows`, `run-ui-windows`, `make-ui-windows` recipes (both `[unix]` no-op stub and `[windows]` real version for each); removed the entire `win-*` recipe family at the bottom (thirteen recipes: `win-bld`, `win-bld-dbg`, `win-bld-dbg-all`, `win-bld-rls`, `win-bld-rls-all`, `win-app-deps`, `win-copy-win`, `win-copy-oth`, `win-app-copy`, `win-app-run`, `win-run-dbg`, `win-run-rls`, `win-total-dbg`, `win-total-rls`); removed the `s` file separator variable, `linux_vulkan_features`, the `os` debugging recipe, and the `set windows-shell` line. `release-intel`, `copy-binary-intel`, `make-ui-intel` are kept.

**On merge conflict:** keep the Mac-only shape. If upstream adds new build recipes, keep only those that make sense on Mac.

### `ui/desktop/README.md`

**What we changed:** rewrote as a short Sponge-focused dev README. Removed the "Platform-specific build requirements" section (Linux `dpkg fakeroot`, Arch, etc.) and the upstream clone URL. Kept: hermit + pnpm workflow, common `pnpm run *` commands, pointer to `just` recipes.

**On merge conflict:** always keep ours. Upstream's README describes cross-platform build; Sponge is Mac-only.

### `BUILDING_LINUX.md` **(deleted)**

Linux build guide.

**On merge conflict (delete/modify):** always keep the delete.

---

## Apple Silicon only (drop Intel Mac)

Sponge distributes only `aarch64-apple-darwin`. Everything Intel-shaped in the fork was deleted or collapsed.

### `.github/workflows/bundle-desktop-intel.yml` **(deleted)**
### `.github/workflows/pr-comment-bundle-intel.yml` **(deleted)**

Intel Mac desktop bundling + its PR-comment download link.

**On merge conflict (delete/modify):** always keep the delete.

### `.github/workflows/release.yml`, `canary.yml`, `bundle-desktop-manual.yml`

**What we changed:** removed the `bundle-desktop-intel` (and `bundle-desktop-intel-unsigned`) job entries and their entries in the release job's `needs:` list.

**On merge conflict:** keep the reduced shape.

### `.github/workflows/build-cli.yml`

**What we changed:** rewrote as a single-target build. The upstream 10-target matrix (Linux gnu/musl/vulkan × aarch64+x86_64, macOS x86_64+aarch64, Windows x86_64 standard+cuda) collapsed to one job on `macos-latest` producing `goose-aarch64-apple-darwin.tar.bz2` and `.tar.gz`. Container/manylinux/Vulkan/CUDA/MSVC steps all gone.

**On merge conflict:** always keep ours. Upstream restructures this matrix regularly; discard.

### `.github/workflows/pr-comment-build-cli.yml`

**What we changed:** the PR-comment body listing CLI download links reduced from ten platform bullets to one (macOS Apple Silicon).

**On merge conflict:** keep the single-link version.

### `justfile`

**What we changed:** removed `release-intel`, `copy-binary-intel`, `make-ui-intel` recipes.

**On merge conflict:** keep the removal.

### `ui/desktop/package.json`

**What we changed:** removed the `bundle:intel` script.

**On merge conflict:** keep the removal.

### `ui/desktop/forge.config.ts`

**What we changed:** `maker-zip` `arch` field is now hardcoded `['arm64']` (was `process.env.ELECTRON_ARCH === 'x64' ? ['x64'] : ['arm64']`).

**On merge conflict:** keep the hardcoded arm64.

### `ui/desktop/scripts/generate-mac-update-manifest.js`

**What we changed:** removed the second entry (`Sponge_intel_mac.zip` → `Sponge-darwin-x64.zip`) from the `files` array. Now only the arm64 entry.

**On merge conflict:** keep the single-entry version.

### `ui/desktop/src/utils/githubUpdater.ts`

**What we changed:** the platform/arch branch inside `getReleaseAsset` replaced with a guard that throws when platform isn't `darwin+arm64`. No more `_intel_mac.zip` / `-win32-x64.zip` / `-linux-*.zip` fallbacks.

**On merge conflict:** keep the guard.

### `download_cli.sh`

**What we changed:** rewrote as an Apple Silicon-only installer. All Linux (gnu/musl/vulkan variants), Windows (standard/cuda variants), and Intel Mac paths removed. Script exits with error if `uname -s -m` is not `Darwin arm64`. `REPO` points at `floatfinancial/goose`. On successful install, calls `goose auth aws-sso` interactively (was `goose configure`).

**On merge conflict:** always keep ours — the upstream script is a multi-platform monster; Sponge's install path is trivially one file, one arch.

### `ui/desktop/README.md`

**What we changed (Apple Silicon additions on top of earlier rewrite):** the platforms line now reads "macOS-only (Apple Silicon)" (was "arm64 + Intel"); removed the `bundle:intel` bullet; removed the `bundle-desktop-intel.yml` reference in the CI note.

**On merge conflict:** keep ours.

---

## Known unpatched surfaces (not blocking, ping if they matter)

- `.github/workflows/python-sdk-wheels.yml` — publishes Python SDK wheels for Linux (manylinux). Sponge doesn't distribute Python bindings. Delete when confirmed no consumer needs them.
- `.github/workflows/ci.yml` — has a Windows cross-compile sanity step (`rustup target add x86_64-pc-windows-msvc`). Not on the distribution path. Trim if it starts failing.

### `.github/workflows/pr-smoke-test.yml` (disabled, not deleted)

**What we changed:** removed the `pull_request` and `push` triggers, leaving only `workflow_dispatch`. The workflow (Live Provider Tests + Compaction Tests + Smoke Tests) now runs only when manually invoked. Header comment explains why and how to re-enable.

Rationale: this workflow hits real Anthropic / OpenAI / Google / Databricks endpoints on every PR to validate goose against the direct provider APIs. Sponge ships only through Bedrock SSO, so Float has no product reason to burn tokens on four providers' bills for every merge. Kept in-place (rather than deleted) in case Float ever wants direct-provider smoke coverage back — restore the two triggers and add `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / `GOOGLE_API_KEY` / `DATABRICKS_HOST` + `DATABRICKS_TOKEN` as repo secrets.

**On merge conflict:** keep our disabled triggers. If upstream restructures the triggers block, port the disable into the new shape.

---

## Adding a new patched file

When a new PR patches an upstream file that isn't listed here:

1. Add a section to this file in the same shape (**What we changed** + **On merge conflict**).
2. Land the manifest update in the same PR as the code change.

The `sync-fork` skill treats an unlisted patched file as a red flag and stops
for human review.
