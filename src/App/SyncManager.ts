import type { TreeNode } from "../lib/types";
import {
  reindex,
  syncSources,
  getTree,
  getSessionCount,
  reindexProviders,
} from "../lib/tauri";
import {
  getProvidersForWatchStrategy,
  loadProviderCatalog,
} from "../stores/providerCatalog";
import { toastError } from "../stores/toast";

export interface SyncCallbacks {
  setTree: (tree: TreeNode[]) => void;
  setSessionCount: (count: number) => void;
  setIsLoading: (loading: boolean) => void;
  syncTabsWithTree: (treeData: TreeNode[]) => void;
}

export function createSyncManager(callbacks: SyncCallbacks) {
  let syncInFlight = false;
  let pendingFullSync = false;
  const pendingChangedPaths = new Set<string>();
  let pollTimer: ReturnType<typeof setInterval> | undefined;

  async function refreshTree() {
    const [treeData, count] = await Promise.all([getTree(), getSessionCount()]);
    callbacks.setTree(treeData);
    callbacks.setSessionCount(count);
    // Sync open tab titles with latest tree data
    callbacks.syncTabsWithTree(treeData);
  }

  async function syncFromDisk(options?: {
    changedPaths?: string[];
    showSpinner?: boolean;
  }) {
    const changedPaths =
      options?.changedPaths?.filter((path) => path.length > 0) ?? [];
    const showSpinner = options?.showSpinner ?? false;

    if (syncInFlight) {
      if (changedPaths.length > 0 && !pendingFullSync) {
        for (const path of changedPaths) pendingChangedPaths.add(path);
      } else {
        pendingFullSync = true;
      }
      return;
    }

    syncInFlight = true;
    if (showSpinner) {
      callbacks.setIsLoading(true);
    }

    try {
      if (changedPaths.length > 0) {
        await syncSources(changedPaths);
      } else {
        await reindex();
      }
      await refreshTree();
    } catch (e) {
      toastError(String(e));
    } finally {
      syncInFlight = false;
      if (showSpinner) {
        callbacks.setIsLoading(false);
      }
      if (pendingFullSync) {
        pendingFullSync = false;
        pendingChangedPaths.clear();
        void syncFromDisk({ showSpinner });
      } else if (pendingChangedPaths.size > 0) {
        const queuedPaths = [...pendingChangedPaths];
        pendingChangedPaths.clear();
        void syncFromDisk({ changedPaths: queuedPaths });
      }
    }
  }

  /** Poll sync — serialized with FS-event sync via syncInFlight guard. */
  async function pollSync(providers: string[]) {
    if (syncInFlight) return;

    syncInFlight = true;
    try {
      await reindexProviders(providers);
      await refreshTree();
    } catch (e) {
      // Polling failures are transient — log for diagnosis, don't toast
      console.debug("poll sync failed:", e);
    } finally {
      syncInFlight = false;
      // Drain pending work (FS events queued during poll take priority)
      if (pendingFullSync) {
        pendingFullSync = false;
        pendingChangedPaths.clear();
        void syncFromDisk();
      } else if (pendingChangedPaths.size > 0) {
        const queuedPaths = [...pendingChangedPaths];
        pendingChangedPaths.clear();
        void syncFromDisk({ changedPaths: queuedPaths });
      }
    }
  }

  async function startPolling() {
    await loadProviderCatalog();
    const pollProviders = getProvidersForWatchStrategy("poll");
    if (pollProviders.length === 0) return;

    pollTimer = setInterval(() => {
      void pollSync(pollProviders);
    }, 5000);
  }

  function stopPolling() {
    clearInterval(pollTimer);
    pollTimer = undefined;
  }

  /** Load cached tree immediately, then reindex in background. */
  async function coldStart() {
    // Show cached data instantly so the user doesn't stare at a spinner
    let cacheHit = false;
    try {
      await refreshTree();
      cacheHit = true;
    } catch {
      // No cached index yet — will be populated by reindex below
    }
    // Only dismiss spinner early on cache hit; keep it up on cache miss
    if (cacheHit) callbacks.setIsLoading(false);

    // Reindex in background
    try {
      await reindex();
      await refreshTree();
    } catch (e) {
      toastError(String(e));
    } finally {
      if (!cacheHit) callbacks.setIsLoading(false);
    }

    await startPolling();
  }

  return {
    syncFromDisk,
    refreshTree,
    coldStart,
    stopPolling,
  };
}
