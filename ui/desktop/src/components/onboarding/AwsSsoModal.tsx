import { useEffect, useRef, useState } from 'react';
import { Cloud, Copy, ExternalLink, Loader2 } from 'lucide-react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '../ui/dialog';
import { Button } from '../ui/button';

interface AwsSsoModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

type Phase = 'loading' | 'form' | 'awaiting' | 'success' | 'error';

interface PromptState {
  verificationUri: string;
  userCode: string;
}

/**
 * AWS SSO sign-in modal. Sends form values to the main process, which spawns
 * `goose auth aws-sso --json` and streams events back over `aws-sso:event`.
 * On success the main process reloads the window so OnboardingGuard re-reads
 * the freshly-written provider defaults.
 */
export default function AwsSsoModal({ open, onOpenChange }: AwsSsoModalProps) {
  const [phase, setPhase] = useState<Phase>('loading');
  const [startUrl, setStartUrl] = useState('');
  const [ssoRegion, setSsoRegion] = useState('us-east-1');
  const [bedrockRegion, setBedrockRegion] = useState('');
  const [prompt, setPrompt] = useState<PromptState | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const cleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    if (!open) {
      cleanupRef.current?.();
      cleanupRef.current = null;
      setPhase('loading');
      setPrompt(null);
      setErrorMessage(null);
      return;
    }

    let cancelled = false;
    void (async () => {
      const defaults = await window.electron.awsSsoGetDefaults();
      if (cancelled) return;
      if (defaults.startUrl) setStartUrl(defaults.startUrl);
      if (defaults.ssoRegion) setSsoRegion(defaults.ssoRegion);
      if (defaults.bedrockRegion) setBedrockRegion(defaults.bedrockRegion);

      // Truly zero-config sign-in: if the team has baked in a start URL + region,
      // skip the form and go straight to the browser.
      if (defaults.startUrl && defaults.ssoRegion) {
        startFlow(defaults.startUrl, defaults.ssoRegion, defaults.bedrockRegion);
      } else {
        setPhase('form');
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!startUrl.trim() || !ssoRegion.trim()) return;
    startFlow(startUrl.trim(), ssoRegion.trim(), bedrockRegion.trim());
  };

  const startFlow = async (url: string, region: string, bedrock: string) => {
    setPhase('awaiting');
    setErrorMessage(null);

    cleanupRef.current = window.electron.onAwsSsoEvent((event) => {
      switch (event.type) {
        case 'prompt':
          setPrompt({
            verificationUri: String(event.verification_uri_complete),
            userCode: String(event.user_code),
          });
          void window.electron.openExternal(String(event.verification_uri_complete));
          break;
        case 'success':
          setPhase('success');
          break;
        case 'error':
          setErrorMessage(String(event.message ?? 'AWS SSO sign-in failed'));
          setPhase('error');
          break;
        case 'exit':
          setPhase((current) => {
            if (current === 'awaiting') {
              setErrorMessage('Sign-in ended unexpectedly. Please retry.');
              return 'error';
            }
            return current;
          });
          break;
      }
    });

    try {
      await window.electron.awsSsoStart({
        startUrl: url,
        ssoRegion: region,
        bedrockRegion: bedrock || region,
      });
    } catch (err) {
      setErrorMessage((err as Error).message);
      setPhase('error');
    }
  };

  const renderBody = () => {
    if (phase === 'loading') {
      return (
        <div className="flex items-center gap-2 text-sm text-text-muted py-8 justify-center">
          <Loader2 size={16} className="animate-spin" />
          Loading…
        </div>
      );
    }

    if (phase === 'awaiting' && prompt) {
      return (
        <div className="flex flex-col gap-4">
          <p className="text-sm text-text-muted">
            A browser tab opened for AWS sign-in. If it didn't, click the link below. Confirm the
            code shown here matches the one in your browser.
          </p>
          <div className="rounded-lg bg-background-muted p-4">
            <div className="text-xs uppercase tracking-wide text-text-muted mb-1">
              Confirmation code
            </div>
            <div className="flex items-center gap-2">
              <code className="text-2xl font-mono tracking-widest">{prompt.userCode}</code>
              <button
                type="button"
                onClick={() => navigator.clipboard.writeText(prompt.userCode)}
                className="text-text-muted hover:text-text-default"
                title="Copy code"
              >
                <Copy size={16} />
              </button>
            </div>
          </div>
          <button
            type="button"
            onClick={() => void window.electron.openExternal(prompt.verificationUri)}
            className="flex items-center gap-2 text-sm text-blue-400 hover:underline"
          >
            <ExternalLink size={14} />
            Open sign-in page
          </button>
          <div className="flex items-center gap-2 text-sm text-text-muted">
            <Loader2 size={14} className="animate-spin" />
            Waiting for you to finish signing in…
          </div>
        </div>
      );
    }

    if (phase === 'awaiting') {
      return (
        <div className="flex items-center gap-2 text-sm text-text-muted py-8 justify-center">
          <Loader2 size={16} className="animate-spin" />
          Contacting AWS SSO…
        </div>
      );
    }

    if (phase === 'success') {
      return (
        <div className="flex flex-col gap-3 py-4 text-sm">
          <p>Signed in and configured for Bedrock. Reloading…</p>
        </div>
      );
    }

    if (phase === 'error') {
      return (
        <div className="flex flex-col gap-3">
          <div className="rounded-lg bg-red-500/10 border border-red-500/30 p-3 text-sm text-red-400">
            {errorMessage}
          </div>
          <Button onClick={() => setPhase('form')} variant="secondary">
            Try again
          </Button>
        </div>
      );
    }

    return (
      <form onSubmit={handleSubmit} className="flex flex-col gap-4">
        <div>
          <label className="text-sm font-medium block mb-1" htmlFor="aws-sso-start-url">
            AWS access portal URL
          </label>
          <input
            id="aws-sso-start-url"
            type="url"
            required
            placeholder="https://your-org.awsapps.com/start"
            value={startUrl}
            onChange={(e) => setStartUrl(e.target.value)}
            className="w-full rounded-md border border-border-default bg-background-default px-3 py-2 text-sm focus:outline-none focus:ring-1 focus:ring-blue-400"
          />
          <p className="text-xs text-text-muted mt-1">
            From your IAM Identity Center dashboard — labelled "AWS access portal URL".
          </p>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="text-sm font-medium block mb-1" htmlFor="aws-sso-region">
              Identity Center region
            </label>
            <input
              id="aws-sso-region"
              type="text"
              required
              placeholder="us-east-1"
              value={ssoRegion}
              onChange={(e) => setSsoRegion(e.target.value)}
              className="w-full rounded-md border border-border-default bg-background-default px-3 py-2 text-sm focus:outline-none focus:ring-1 focus:ring-blue-400"
            />
          </div>
          <div>
            <label className="text-sm font-medium block mb-1" htmlFor="aws-bedrock-region">
              Bedrock region
            </label>
            <input
              id="aws-bedrock-region"
              type="text"
              placeholder="(defaults to Identity Center region)"
              value={bedrockRegion}
              onChange={(e) => setBedrockRegion(e.target.value)}
              className="w-full rounded-md border border-border-default bg-background-default px-3 py-2 text-sm focus:outline-none focus:ring-1 focus:ring-blue-400"
            />
          </div>
        </div>
        <Button type="submit">Sign in with AWS SSO</Button>
      </form>
    );
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Cloud size={18} />
            Sign in with AWS SSO
          </DialogTitle>
        </DialogHeader>
        {renderBody()}
      </DialogContent>
    </Dialog>
  );
}
