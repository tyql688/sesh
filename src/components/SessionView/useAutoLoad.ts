import { createEffect } from "solid-js";
import type { Accessor } from "solid-js";

export interface UseAutoLoadOptions<T> {
  /** Tracked: re-run when the visible entry list changes. */
  visibleEntries: Accessor<T[]>;
  loading: Accessor<boolean>;
  hasMore: Accessor<boolean>;
  /** Lazy ref getter — the DOM node may not exist on first run. */
  getMessagesRef: () => HTMLDivElement | undefined;
  loadMore: () => void;
  threshold: number;
}

/**
 * Automatically calls `loadMore` when the scroll container's content doesn't
 * fill the viewport and more entries are available — covers the initial-mount
 * case where the default window is smaller than the viewport height.
 */
export function useAutoLoad<T>(opts: UseAutoLoadOptions<T>): void {
  createEffect(() => {
    opts.visibleEntries();
    const ref = opts.getMessagesRef();
    if (opts.loading() || !opts.hasMore() || !ref) return;

    if (ref.scrollHeight <= ref.clientHeight + opts.threshold) {
      requestAnimationFrame(() => {
        opts.loadMore();
      });
    }
  });
}
