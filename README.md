# Sponge 🧽

Sponge is Float's internal AI agent — a desktop app for knowledge workers and a CLI for developers. It's a downstream distribution of the open-source [goose](https://github.com/aaif-goose/goose) project by AAIF (Agentic AI Foundation), rebranded and preconfigured for Float.

**Internal use only.** Not for external distribution.

## Install

### Desktop (knowledge workers)

Delivered via MDM. First launch prompts for AWS SSO sign-in — no API keys, no config. The app will auto-update from the latest release published to this repo.

### CLI (developers)

Download the latest `goose` binary from [Releases](https://github.com/floatfinancial/goose/releases), then:

```bash
goose auth aws-sso    # one-time SSO sign-in
goose session         # start an interactive session
```

The CLI binary is named `goose` (unchanged from upstream) to keep muscle memory and scripts working.

## What's different from upstream

- **Zero-config AWS Bedrock via IAM Identity Center SSO** — see `crates/goose/src/providers/aws_sso.rs` and `bedrock_float.rs`.
- **Rebranded as Sponge** for the desktop app — see `docs/float-fork/PATCHES.md`.
- **Telemetry disabled at source** — see `crates/goose/src/posthog.rs`.
- **Preconfigured provider tiles** — `ui/desktop/src/distroConfig.ts` sets defaults for the Float Identity Center start URL and Bedrock region.

Everything else tracks upstream. We periodically sync via `just sync-fork` (see `.pi/skills/sync-fork/SKILL.md`).

## Working on the fork

- `AGENTS.md` — coding rules for this repo.
- `CUSTOM_DISTROS.md` — the goose customization guide.
- `docs/float-fork/PATCHES.md` — manifest of every upstream file Float patches, and how to resolve merge conflicts.
- `BUILDING_LINUX.md`, `BUILDING_DOCKER.md` — platform build notes.
- `I18N.md` — how translations flow.

Common commands:

```bash
just release-binary       # build the CLI
just run-ui-only          # run the desktop UI against the last-built CLI
just check-everything     # fmt + clippy + UI lint
```

## Reporting bugs

Internal only — [open an issue in this repo](https://github.com/floatfinancial/goose/issues/new?template=bug_report.md). The desktop app's Settings → Diagnostics screen exports a diagnostics zip; attach it.

## License and attribution

Sponge is Apache 2.0, inherited from upstream goose. Every source file's original copyright notice is preserved. See `LICENSE`.

Sponge is not endorsed by AAIF and is not the "official" goose. See [upstream goose](https://github.com/aaif-goose/goose) for the source project.
