/**
 * Float fork additions to the ACP provider wrappers.
 *
 * Kept in a separate file so upstream `acp/providers.ts` merges cleanly. The
 * only things that file touches from us are:
 * - one import line at the top
 * - one call to `filterVisibleProviders(entries)` inside `acpListProviderDetails`
 * - one call to `enhanceProviderModels(client, providerId, staticModels)` at
 *   the tail of `acpListProviderModels`
 *
 * Everything else — the visible-provider set, the runtime enumeration + fallback
 * — is owned here. See `docs/float-fork/PATCHES.md`.
 */

import type { GooseClient } from '@aaif/goose-sdk';
import { VISIBLE_PROVIDERS } from '../distroConfig';

type ProviderEntry = { providerId: string; [k: string]: unknown };
type ModelEntry = { id: string; [k: string]: unknown };

const visibleProviderSet: Set<string> = new Set(VISIBLE_PROVIDERS);

/**
 * Drop providers the current distro doesn't surface in the tile grid. Every
 * registered Rust provider is still reachable via config / env; this only
 * controls the UI. Edit `VISIBLE_PROVIDERS` in `distroConfig.ts` to un-hide.
 */
export function filterVisibleProviders<T extends ProviderEntry>(entries: T[]): T[] {
  return entries.filter((entry) => visibleProviderSet.has(entry.providerId));
}

/**
 * Ask the provider to enumerate its models at runtime (Bedrock lists what the
 * current AWS SSO identity can actually invoke) and merge with the static
 * inventory. Preserves context limits / reasoning flags from `staticModels`
 * whenever a runtime id has a static match; falls back to `staticModels` on
 * any error.
 */
export async function enhanceProviderModels<M extends ModelEntry>(
  client: GooseClient,
  providerId: string,
  staticModels: M[]
): Promise<M[]> {
  try {
    const { models: runtimeIds } = await client.goose.providersSupportedModelsList_unstable({
      providerId,
    });
    if (runtimeIds.length === 0) return staticModels;

    const byId = new Map(staticModels.map((m) => [m.id, m]));
    // Fallback shape: `{ id, name }`. All other `ProviderInventoryModelDto` fields
    // are optional so this is structurally assignable to `M` when M is that DTO.
    // For strictly-typed M with required fields beyond id/name, callers should
    // pre-populate `staticModels` with those ids.
    return runtimeIds.map((id) => (byId.get(id) ?? ({ id, name: id } as unknown as M)));
  } catch (err) {
    console.warn(`providersSupportedModelsList_unstable failed for ${providerId}:`, err);
    return staticModels;
  }
}
