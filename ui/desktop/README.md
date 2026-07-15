# Sponge Desktop App

Electron/React desktop app. Downstream of upstream goose (see project root `README.md`).

## Setup

```bash
git clone git@github.com:floatfinancial/goose.git
cd goose
source ./bin/activate-hermit
cd ui/desktop
pnpm install
pnpm run start
```

Sponge is macOS-only (Apple Silicon).

## Common commands

- `pnpm run start-gui` — dev launch (rebuilds CLI, then starts Electron)
- `pnpm run make` — package the `.app`
- `pnpm run bundle:default` — arm64 `.app` + `.zip` for GitHub Release
- `pnpm run lint:check` / `pnpm run typecheck` / `pnpm test`

From the repo root:

- `just release-binary` — build the Rust CLI + copy into `src/bin/`
- `just run-ui-only` — start the UI without rebuilding the CLI
- `just package-ui` — build + ad-hoc sign a local `.app` (Gatekeeper won't accept; use only for local smoke tests)

Signed / notarized builds happen in CI (`.github/workflows/bundle-desktop.yml`) when `APPLE_TEAM_ID` is set.
