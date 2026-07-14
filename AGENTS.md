# AGENTS Instructions

goose is an AI agent framework in Rust with a CLI, a desktop (Electron) GUI, and a terminal (Ink) TUI. The Rust core exposes itself over the **Agent Client Protocol (ACP)**; every frontend is an ACP client talking to the same `goose` binary.

## Setup
```bash
source bin/activate-hermit
cargo build
```

## Commands

### Build
```bash
cargo build                          # debug
cargo build --release                # release
just release-binary                  # release binary, copied to ui/desktop/src/bin
just copy-binary [BUILD_MODE]        # copy target/<mode>/goose into the desktop app
```

### Test
```bash
cargo test                                                  # workspace
cargo test -p goose                                         # one crate
cargo test --package goose --test mcp_integration_test      # one test
just record-mcp-tests                                       # record MCP fixtures
```

### Lint / format
```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
just check-everything                # fmt + clippy + UI lint
```

### UI
```bash
just run-ui              # builds release binary, launches Electron desktop
just run-ui-only         # skip Rust build; use whatever binary is already staged
just run-server          # run `goose serve` (ACP over HTTP/WS) on :3000
cd ui/desktop && pnpm run typecheck
cd ui/desktop && pnpm test
cd ui/text && pnpm start # Ink terminal UI against released binary
```

## Repository map

```
crates/
├── goose                    # core: agent loop, ACP server, sessions, providers glue,
│                              recipes, scheduler, security, context mgmt
├── goose-acp-macros         # proc macros for ACP request/notification wiring
├── goose-cli                # `goose` binary: subcommands, TUI session, ACP serve
├── goose-download-manager   # background downloads (models, extensions)
├── goose-local-inference    # llama.cpp / MLX local model backends
├── goose-mcp                # bundled MCP servers: developer, memory, tutorial,
│                              computercontroller, autovisualiser, peekaboo
├── goose-provider-types     # provider trait + shared types (messages, tools, usage)
├── goose-providers          # concrete providers: anthropic, openai, google,
│                              ollama, databricks, snowflake, openai_compatible…
├── goose-sdk                # embeddable SDK + optional uniffi bindings (py/kotlin)
├── goose-sdk-types          # shared types for the SDK surface
├── goose-test / -support    # integration test harness and fixtures

ui/
├── desktop                  # Electron + React GUI (the "Goose Desktop" app)
├── text                     # Ink React TUI (goose-tui, ACP client)
├── sdk                      # TypeScript SDK, generated from ACP schema
├── goose-binary             # per-platform prebuilt binary packages
└── install-link-generator   # deeplink installer helper

documentation/               # docusaurus site (goose-docs.ai)
examples/                    # runnable demos
evals/                       # eval harness
scripts/, services/, workflow_recipes/, recipe-scanner/
```

### Entry points
- **CLI binary** — `crates/goose-cli/src/main.rs` → `cli.rs` (clap subcommands)
- **Agent loop** — `crates/goose/src/agents/agent.rs` (`Agent`, `AgentEvent`)
- **ACP server** — `crates/goose/src/acp/` (`server/`, `server_factory.rs`, `transport/`)
- **Bundled MCP servers** — `crates/goose-mcp/src/*/` behind `mcp_server_runner`
- **Desktop main process** — `ui/desktop/src/main.ts` (Electron; spawns `goose serve`)
- **Desktop renderer** — `ui/desktop/src/App.tsx` and `ui/desktop/src/acp/`
- **Terminal UI** — `ui/text/src/tui.tsx` (spawns `goose acp` over stdio)

## Key CLI subcommands

`goose <cmd>` — see `crates/goose-cli/src/cli.rs`:
- `session` (alias `s`) — interactive TUI chat, resume, export/import, diagnostics
- `run` — non-interactive: execute an instruction file, stdin, or recipe
- `acp` — run as an ACP agent over **stdio** (used by the Ink TUI and editors)
- `serve` — run ACP over **HTTP + WebSocket** (used by the desktop app)
- `mcp <server>` — expose one of the bundled MCP servers on stdio
- `configure` / `info` / `doctor` — setup, config dump, health check
- `recipe` — validate / open / deeplink / list recipes
- `schedule` — cron jobs (add/list/remove/run-now/sessions)
- `gateway` — pairing + remote gateway (telegram bridge, etc.)
- `plugin`, `skills`, `project`, `term`, `update` — auxiliary tooling

## Core concepts

- **Agent** — orchestrates one turn: send messages to a `Provider`, receive tool calls, run them through `ExtensionManager`, stream `AgentEvent`s back. Handles retries, permission prompts, compaction, subagents.
- **Provider** — pluggable LLM backend implementing the trait in `goose-provider-types/src/base.rs`. All concrete providers live in `crates/goose-providers/src/`.
- **Extension / MCP** — capabilities are exposed as MCP servers. Built-ins live in `crates/goose-mcp`; external ones are spawned as subprocesses or connected over stdio/http via `mcp_client.rs`. `ExtensionManager` (in `crates/goose/src/agents/extension_manager.rs`) is the registry.
- **ACP (Agent Client Protocol)** — the wire protocol every frontend uses. Two transports:
  - **stdio** (`goose acp`) — for the Ink TUI, editors, tests
  - **HTTP+WebSocket** (`goose serve`) — for the desktop app; auth via `GOOSE_SERVER__SECRET_KEY`
- **Session** — persisted conversation state; managed by `SessionManager` (`crates/goose/src/session/session_manager.rs`). Types include chat, run, subagent.
- **Recipe** — a declarative agent config (prompt, tools, settings). Loaded via `crates/goose/src/recipe/` and CLI helpers in `crates/goose-cli/src/recipes/`.
- **Scheduler** — cron-driven recipe runs. `SchedulerTrait` in `crates/goose/src/scheduler_trait.rs`.
- **Security / permissions** — `crates/goose/src/security/` (adversary + egress inspectors) and `crates/goose/src/permission/` (permission judge for tool calls).
- **Context management** — automatic compaction and token budgeting in `crates/goose/src/context_mgmt/`.
- **Skills / hints / hooks** — user-authored guidance loaded from `.goosehints`, skill dirs, and lifecycle hooks (`crates/goose/src/hooks/`, `skills/`, `hints/`).
- **Subagents** — nested agent runs (`agents/subagent_handler.rs`, `subagent_execution_tool/`).

## Frontends in one paragraph each

**Desktop (`ui/desktop`)** — Electron main process (`main.ts`) spawns `goose serve` on a random loopback port with a generated secret key, tracks it via `GooseServeLeaseRegistry`, and the React renderer (`App.tsx` + `src/acp/*`) connects over WebSocket. Uses `@aaif/goose-sdk` (`ui/sdk`) for the typed ACP client. Routes are hash-based; the main views are Hub, Pair, Sessions, Schedules, Settings. Built-in extension defaults live in `ui/desktop/src/built-in-extensions.json`.

**Terminal UI (`ui/text`)** — Ink + React. `tui.tsx` spawns `goose acp` over stdio (binary resolved from `GOOSE_BINARY` or the pinned `@aaif/goose-binary-*` package) and renders a chat UI. Constraints: Ink cannot clip — see `AGENTS.md` in `ui/text/` (and the Ink notes in this file's history) for layout rules.

**CLI TUI (`crates/goose-cli/src/session/`)** — the classic `goose session` interactive prompt. Uses cliclack for prompts, ratatui/console for rendering; not the same code as `ui/text`.

## Development loop
```bash
# 1. source bin/activate-hermit
# 2. Make changes
# 3. cargo fmt
```

Run these only when the user asks you to build/test:
```bash
cargo build
cargo test -p <crate>
cargo clippy --all-targets -- -D warnings
```

## Rules

- Tests: prefer `crates/<crate>/tests/` over inline `#[cfg(test)]` when the surface is public
- Tests: when adding features, update `goose-self-test.yaml`, rebuild, then `goose run --recipe goose-self-test.yaml` to validate
- Errors: `anyhow::Result` at boundaries, `thiserror` for typed errors
- Providers: implement the `Provider` trait in `goose-provider-types/src/base.rs`
- MCP: new extensions go in `crates/goose-mcp/`
- UI Desktop: use ACP SDK types (`@aaif/goose-sdk`) or local `src/types/*`. Do **not** import generated OpenAPI code from `ui/desktop/src/api`.

## Code quality

- Self-documenting names beat comments; comment only "why", never "what"
- Don't make things optional when the compiler can enforce them; booleans default to `false`, not `Option<bool>`
- Skip decorative error context (`.context("Failed to X")` when the underlying error already says so)
- Trust the type system — avoid defensive branches
- Prune logs; only add for real errors or security events

## Ink / terminal UI rules (`ui/text`)

- Ink does not clip. Overflowing text bleeds into neighbouring cells.
- Never `wrap="wrap"` inside a fixed-height Box; use `wrap="truncate"` and pre-truncate to `lines × width`.
- Account for borders (2), padding, margins, and siblings when computing available space.
- Avoid `flexGrow={1}` on text containers inside fixed-height cards.
- Audit every line of chrome when computing the height budget.
- Don't apply `marginBottom` to the last item; use `gap` on the container.

## Never

- Recreate `ui/desktop/src/api` or add `@hey-api/openapi-ts` to `ui/desktop`
- Manually edit `Cargo.toml` dependency entries for human-authored changes — use `cargo add` (automated bump PRs exempt; keep `Cargo.lock` consistent)
- Skip `cargo fmt`
- Merge without clippy
- Comment self-evident operations, getters/setters, constructors, or standard Rust idioms
