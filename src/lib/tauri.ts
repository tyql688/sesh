import { invoke } from "@tauri-apps/api/core";
import type {
  SessionDetail,
  SearchResult,
  SearchFilters,
  TreeNode,
  IndexStats,
  ProviderSnapshot,
  TrashMeta,
  SessionMeta,
} from "./types";

export async function reindex(): Promise<number> {
  return invoke<number>("reindex");
}

export async function reindexProviders(providers: string[]): Promise<number> {
  return invoke<number>("reindex_providers", { providers });
}

export async function syncSources(paths: string[]): Promise<number> {
  return invoke<number>("sync_sources", { paths });
}

export async function getTree(): Promise<TreeNode[]> {
  return invoke<TreeNode[]>("get_tree");
}

export async function getSessionDetail(
  sessionId: string,
): Promise<SessionDetail> {
  return invoke<SessionDetail>("get_session_detail", { sessionId });
}

export async function searchSessions(
  filters: SearchFilters,
): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("search_sessions", { filters });
}

export async function renameSession(
  sessionId: string,
  newTitle: string,
): Promise<void> {
  return invoke<void>("rename_session", { sessionId, newTitle });
}

export async function getSessionCount(): Promise<number> {
  return invoke<number>("get_session_count");
}

export async function exportSession(
  sessionId: string,
  format: string,
  outputPath: string,
): Promise<void> {
  return invoke<void>("export_session", {
    sessionId,
    format,
    outputPath,
  });
}

export async function getChildSessions(
  parentId: string,
): Promise<SessionMeta[]> {
  return invoke<SessionMeta[]>("get_child_sessions", { parentId });
}

export async function getIndexStats(): Promise<IndexStats> {
  return invoke<IndexStats>("get_index_stats");
}

export async function rebuildIndex(): Promise<number> {
  return invoke<number>("rebuild_index");
}

export async function clearIndex(): Promise<void> {
  return invoke<void>("clear_index");
}

export async function detectTerminal(): Promise<string> {
  return invoke<string>("detect_terminal");
}

export async function getProviderSnapshots(): Promise<ProviderSnapshot[]> {
  return invoke<ProviderSnapshot[]>("get_provider_snapshots");
}

export async function openInTerminal(
  command: string,
  cwd: string | null,
  terminalApp: string,
): Promise<void> {
  return invoke<void>("open_in_terminal", { command, cwd, terminalApp });
}

export async function resumeSession(
  sessionId: string,
  terminalApp: string,
): Promise<void> {
  return invoke<void>("resume_session", { sessionId, terminalApp });
}

export async function getResumeCommand(sessionId: string): Promise<string> {
  return invoke<string>("get_resume_command", { sessionId });
}

export async function trashSession(sessionId: string): Promise<void> {
  return invoke<void>("trash_session", { sessionId });
}

export async function listTrash(): Promise<TrashMeta[]> {
  return invoke<TrashMeta[]>("list_trash");
}

export async function restoreSession(trashId: string): Promise<void> {
  return invoke<void>("restore_session", { trashId });
}

export async function emptyTrash(): Promise<void> {
  return invoke<void>("empty_trash");
}

export async function permanentDeleteTrash(trashId: string): Promise<void> {
  return invoke<void>("permanent_delete_trash", { trashId });
}

export async function listRecentSessions(
  limit: number,
): Promise<SessionMeta[]> {
  return invoke<SessionMeta[]>("list_recent_sessions", { limit });
}

export async function toggleFavorite(sessionId: string): Promise<boolean> {
  return invoke<boolean>("toggle_favorite", { sessionId });
}

export async function listFavorites(): Promise<SessionMeta[]> {
  return invoke<SessionMeta[]>("list_favorites");
}

export async function isFavorite(sessionId: string): Promise<boolean> {
  return invoke<boolean>("is_favorite", { sessionId });
}

export async function readImageBase64(path: string): Promise<string> {
  return invoke<string>("read_image_base64", { path });
}

export async function openInFolder(path: string): Promise<void> {
  return invoke<void>("open_in_folder", { path });
}

export async function exportSessionsBatch(
  items: string[],
  format: string,
  outputPath: string,
): Promise<void> {
  return invoke<void>("export_sessions_batch", { items, format, outputPath });
}
