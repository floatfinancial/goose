/**
 * Distribution-specific constants for this build of Goose.
 *
 * Every value that changes per fork lives here. Custom distributions
 * (see CUSTOM_DISTROS.md) should edit only this file plus a small handful
 * of assets — the rest of the codebase reads from here.
 *
 * Nothing in this file is a secret. Values are visible in the packaged app.
 */

/**
 * Defaults for the "Sign in with AWS SSO" flow. When both `startUrl` and
 * `ssoRegion` are set, the modal skips its form and takes the user straight
 * to the browser — the one-click sign-in path.
 *
 * Environment variables win if set, so a single build can still be pointed
 * at another Identity Center instance without rebuilding.
 */
export interface AwsSsoDefaults {
  startUrl: string;
  ssoRegion: string;
  bedrockRegion: string;
}

export const AWS_SSO_DEFAULTS: AwsSsoDefaults = {
  startUrl: process.env.GOOSE_AWS_SSO_START_URL ?? 'https://floatcard.awsapps.com/start',
  ssoRegion: process.env.GOOSE_AWS_SSO_REGION ?? 'us-east-2',
  bedrockRegion: process.env.GOOSE_AWS_BEDROCK_REGION ?? 'us-east-2',
};

/**
 * Providers surfaced in the tile grid. Every registered Rust provider is
 * still reachable via config / env; this only controls the UI. Add an id
 * here to un-hide a tile.
 */
export const VISIBLE_PROVIDERS: readonly string[] = [
  'aws_bedrock',
  'ollama',
  'claude-acp', // Claude Code
  'claude-code', // Claude Code CLI
  'cursor-agent',
  'google', // Google Gemini (API Key) — works with AI Studio keys for Workspace users
  'copilot-acp', // GitHub Copilot CLI
  'local',
  'pi-acp',
  'ollama_cloud',
  'openrouter',
];
