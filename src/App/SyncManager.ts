import type { TreeNode } from "../lib/types";
import { reindex, syncSources, getTree, getSessionCount } from "../lib/tauri";
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

  /** Load cached tree immediately, then reindex in background. */
  async function coldStart() {
    // Show cached data instantly so the user doesn't stare at a spinner
    try {
      await refreshTree();
    } catch {
      // No cached index yet — will be populated by reindex below
    }
    callbacks.setIsLoading(false);

    // Reindex in background (no spinner)
    try {
      await reindex();
      await refreshTree();
    } catch (e) {
      toastError(String(e));
    }
  }

  return {
    syncFromDisk,
    refreshTree,
    coldStart,
  };
}
