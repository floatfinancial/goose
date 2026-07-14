import { spawn } from 'node:child_process';
import { BrowserWindow, ipcMain } from 'electron';
import log from './utils/logger';
import { findGooseBinaryPath } from './gooseServe';
import { AWS_SSO_DEFAULTS } from './distroConfig';

export interface AwsSsoStartArgs {
  startUrl: string;
  ssoRegion: string;
  bedrockRegion?: string;
  model?: string;
}

/** Newline-delimited JSON events the CLI emits on stdout when `--json` is set. */
export type AwsSsoEvent =
  | {
      type: 'prompt';
      verification_uri_complete: string;
      user_code: string;
      expires_in: number;
    }
  | { type: 'success'; summary: Record<string, unknown> }
  | { type: 'error'; message: string }
  | { type: 'exit'; code: number | null };

const EVENT_CHANNEL = 'aws-sso:event';

interface RegisterOptions {
  isPackaged: boolean;
  resourcesPath: string;
}

export function registerAwsSsoIpc({ isPackaged, resourcesPath }: RegisterOptions): void {
  // Hardcoded defaults so the whole team gets one-click sign-in without any
  // shell / env setup. Environment variables win if set, so branches / other
  // teams can point elsewhere without rebuilding. See distroConfig.ts.
  ipcMain.handle('aws-sso:get-defaults', () => ({ ...AWS_SSO_DEFAULTS }));

  ipcMain.handle('aws-sso:start', async (event, args: AwsSsoStartArgs) => {
    const sender = event.sender;
    const win = BrowserWindow.fromWebContents(sender);
    if (!args.startUrl || !args.ssoRegion) {
      sender.send(EVENT_CHANNEL, {
        type: 'error',
        message: 'startUrl and ssoRegion are required',
      });
      return { ok: false };
    }

    let binaryPath: string;
    try {
      binaryPath = findGooseBinaryPath({ isPackaged, resourcesPath });
    } catch (err) {
      sender.send(EVENT_CHANNEL, {
        type: 'error',
        message: `Could not locate the goose binary: ${(err as Error).message}`,
      });
      return { ok: false };
    }

    const cliArgs = [
      'auth',
      'aws-sso',
      '--json',
      '--start-url',
      args.startUrl,
      '--sso-region',
      args.ssoRegion,
      '--bedrock-region',
      args.bedrockRegion ?? args.ssoRegion,
    ];
    if (args.model) cliArgs.push('--model', args.model);

    log.info(`[aws-sso] spawning ${binaryPath} ${cliArgs.slice(0, 3).join(' ')} …`);
    const child = spawn(binaryPath, cliArgs, { stdio: ['ignore', 'pipe', 'pipe'] });

    let stdoutBuf = '';
    child.stdout.on('data', (chunk: Buffer) => {
      stdoutBuf += chunk.toString('utf8');
      let idx: number;
      while ((idx = stdoutBuf.indexOf('\n')) !== -1) {
        const line = stdoutBuf.slice(0, idx).trim();
        stdoutBuf = stdoutBuf.slice(idx + 1);
        if (!line) continue;
        try {
          const parsed = JSON.parse(line) as AwsSsoEvent;
          sender.send(EVENT_CHANNEL, parsed);
        } catch {
          log.warn(`[aws-sso] non-JSON stdout line: ${line}`);
        }
      }
    });

    let stderrBuf = '';
    child.stderr.on('data', (chunk: Buffer) => {
      stderrBuf += chunk.toString('utf8');
    });

    child.on('exit', (code) => {
      if (stderrBuf.trim()) log.warn(`[aws-sso] stderr: ${stderrBuf.trim()}`);
      sender.send(EVENT_CHANNEL, { type: 'exit', code });
      if (code === 0 && win) {
        // Config is on disk; reload so OnboardingGuard re-reads defaults.
        win.webContents.reload();
      }
    });

    return { ok: true };
  });
}
