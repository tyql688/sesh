import { createEffect, on, onCleanup } from "solid-js";
import type { Accessor } from "solid-js";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  getProviderWatchConfig,
  getProviderWatchVersion,
  loadProviderWatchSnapshots,
} from "../../lib/provider-watch";
import type { Provider } from "../../lib/types";

export interface UseLiveWatchOptions {
  watching: Accessor<boolean>;
  provider: Accessor<Provider>;
  sourcePath: Accessor<string>;
  reload: () => Promise<void>;
}

/**
 * Manages the live-watch subscription for a session. When `watching` is true,
 * either polls (for DB-backed providers like OpenCode) or subscribes to the
 * `sessions-changed` FS event. All timers and unlisten fns are owned here so
 * the parent component doesn't juggle them inline.
 */
export function useLiveWatch(opts: UseLiveWatchOptions): void {
  let unwatchFn: UnlistenFn | undefined;
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  let watchDebounce: ReturnType<typeof setTimeout> | undefined;

  createEffect(
    on(
      () =>
        [
          opts.watching(),
          opts.provider(),
          opts.sourcePath(),
          getProviderWatchVersion(),
        ] as const,
      async ([isWatching]) => {
        clearTimeout(watchDebounce);
        clearInterval(pollTimer);
        pollTimer = undefined;
        unwatchFn?.();
        unwatchFn = undefined;

        if (!isWatching) return;

        void loadProviderWatchSnapshots();

        const activeSourcePath = opts.sourcePath();
        const watchConfig = getProviderWatchConfig(opts.provider());

        if (watchConfig.strategy === "poll") {
          pollTimer = setInterval(
            () => void opts.reload(),
            watchConfig.debounceMs,
          );
          return;
        }

        unwatchFn = await listen<string[]>("sessions-changed", (event) => {
          const changedPaths = event.payload ?? [];
          if (!activeSourcePath) return;

          let matched: boolean;
          if (watchConfig.matchPrefix) {
            // Gemini: match by project directory prefix
            // (strip last 2 path segments: /chats/session-id.json → project dir)
            const dir = activeSourcePath.replace(/\/[^/]+\/[^/]+$/, "");
            matched = changedPaths.some((p) => p.startsWith(dir));
          } else {
            matched = changedPaths.includes(activeSourcePath);
          }
          if (!matched) return;

          clearTimeout(watchDebounce);
          watchDebounce = setTimeout(
            () => void opts.reload(),
            watchConfig.debounceMs,
          );
        });
      },
    ),
  );

  onCleanup(() => {
    clearTimeout(watchDebounce);
    clearInterval(pollTimer);
    pollTimer = undefined;
    unwatchFn?.();
  });
}
