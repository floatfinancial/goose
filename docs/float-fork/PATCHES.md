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

**On merge conflict:** all four additions are independent inserts. Reapply each in the new file location.

### `crates/goose-cli/src/commands/mod.rs`

**What we changed:** added `pub mod auth;` at the top.

**On merge conflict:** keep the line.

### `crates/goose/Cargo.toml`

**What we changed:** added AWS SSO / STS / SDK deps + feature list entries for the `aws-providers` feature. See git log for the exact set.

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

## Adding a new patched file

When a new PR patches an upstream file that isn't listed here:

1. Add a section to this file in the same shape (**What we changed** + **On merge conflict**).
2. Land the manifest update in the same PR as the code change.

The `sync-fork` skill treats an unlisted patched file as a red flag and stops
for human review.
