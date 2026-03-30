import type { MenuItemDef } from "../ContextMenu";
import type { TreeNode } from "../../lib/types";
import { getProviderConfig } from "../../lib/providers";
import { openInFolder } from "../../lib/tauri";
import { toast, toastError } from "../../stores/toast";
import { selectionCount } from "../../stores/selection";
import { bumpFavoriteVersion } from "../../stores/favorites";

export interface SessionMenuContext {
  node: TreeNode;
  sessionProjectPath: string;
  providerLabel: string | undefined;
  t: (key: string) => string;
  terminalApp: string;
  resumeSession: (
    id: string,
    provider: string,
    terminal: string,
  ) => Promise<void>;
  toggleFavorite: (id: string) => Promise<boolean>;
  setRenameTarget: (target: { id: string; label: string }) => void;
  onExportSession?: (id: string) => void;
  onDeleteSession?: (id: string) => void;
}

export function buildSessionMenuItems(ctx: SessionMenuContext): MenuItemDef[] {
  const { node, sessionProjectPath, providerLabel, t } = ctx;
  const items: MenuItemDef[] = [
    {
      label: t("contextMenu.copySessionId"),
      onClick: () => {
        void navigator.clipboard
          .writeText(node.id)
          .then(() => toast(t("toast.idCopied")));
      },
    },
    {
      label: t("contextMenu.copyResumeCommand"),
      onClick: () => {
        const provider = node.provider ?? "claude";
        let cmd: string;
        if (provider === "cc-mirror" && providerLabel) {
          cmd = `${providerLabel} --resume ${node.id}`;
        } else {
          const config = getProviderConfig(provider);
          cmd = config.resumeCommand(node.id);
        }
        void navigator.clipboard
          .writeText(cmd)
          .then(() => toast(t("toast.cmdCopied")));
      },
    },
    ...(sessionProjectPath
      ? [
          {
            label: t("contextMenu.openInFinder"),
            onClick: () => {
              openInFolder(sessionProjectPath).catch(() => {});
            },
          },
          {
            label: t("contextMenu.copyPath"),
            onClick: () => {
              void navigator.clipboard
                .writeText(sessionProjectPath)
                .then(() => toast(t("toast.copied")));
            },
          },
        ]
      : []),
    { label: "", separator: true, onClick: () => {} },
    {
      label: t("contextMenu.resumeSession"),
      onClick: async () => {
        const provider = node.provider ?? "claude";
        await ctx.resumeSession(node.id, provider, ctx.terminalApp);
      },
    },
    { label: "", separator: true, onClick: () => {} },
    {
      label: t("contextMenu.toggleFavorite"),
      onClick: async () => {
        try {
          const newState = await ctx.toggleFavorite(node.id);
          bumpFavoriteVersion();
          toast(t(newState ? "toast.favoriteAdded" : "toast.favoriteRemoved"));
        } catch (_e) {
          toastError(t("toast.favoriteFailed"));
        }
      },
    },
    {
      label: t("contextMenu.rename"),
      onClick: () => {
        ctx.setRenameTarget({ id: node.id, label: node.label });
      },
    },
    { label: "", separator: true, onClick: () => {} },
  ];
  if (ctx.onExportSession) {
    items.push({
      label: t("contextMenu.export"),
      onClick: () => ctx.onExportSession?.(node.id),
    });
  }
  if (ctx.onDeleteSession) {
    items.push({
      label: t("contextMenu.delete"),
      onClick: () => ctx.onDeleteSession?.(node.id),
    });
  }
  return items;
}

export interface SelectionMenuContext {
  t: (key: string) => string;
  trashSelected: () => void;
  exportSelectedBatch: () => void;
}

export function buildSelectionMenuItems(
  ctx: SelectionMenuContext,
): MenuItemDef[] {
  return [
    {
      label: () =>
        `${ctx.t("contextMenu.deleteSelected")} (${selectionCount()})`,
      onClick: ctx.trashSelected,
    },
    {
      label: () =>
        `${ctx.t("contextMenu.exportSelected")} (${selectionCount()})`,
      onClick: ctx.exportSelectedBatch,
    },
  ];
}

export interface NodeMenuContext {
  node: TreeNode;
  t: (key: string) => string;
  collapseAllChildren: (node: TreeNode) => void;
  expandAllChildren: (node: TreeNode) => void;
  collapseNode: (nodeId: string) => void;
  trashAllUnderNode: (node: TreeNode) => void;
  onRefreshTree?: () => void;
  addBlockedFolder: (path: string) => void;
}

export function buildNodeMenuItems(ctx: NodeMenuContext): MenuItemDef[] {
  const { node, t } = ctx;
  if (node.node_type === "provider") {
    return [
      {
        label: t("contextMenu.collapseAll"),
        onClick: () => ctx.collapseAllChildren(node),
      },
      {
        label: t("contextMenu.refresh"),
        onClick: () => ctx.onRefreshTree?.(),
      },
      { label: "", separator: true, onClick: () => {} },
      {
        label: t("contextMenu.deleteAll"),
        onClick: () => ctx.trashAllUnderNode(node),
      },
    ];
  }
  const projectPath = node.project_path ?? "";
  const hasPath = projectPath.length > 0;
  return [
    ...(hasPath
      ? [
          {
            label: t("contextMenu.openInFinder"),
            onClick: () => {
              openInFolder(projectPath).catch(() => {});
            },
          },
          {
            label: t("contextMenu.copyPath"),
            onClick: () => {
              void navigator.clipboard
                .writeText(projectPath)
                .then(() => toast(t("toast.copied")));
            },
          },
          { label: "", separator: true, onClick: () => {} },
        ]
      : []),
    {
      label: t("contextMenu.expandAll"),
      onClick: () => ctx.expandAllChildren(node),
    },
    {
      label: t("contextMenu.collapseAll"),
      onClick: () => ctx.collapseNode(node.id),
    },
    ...(hasPath
      ? [
          {
            label: t("contextMenu.blockFolder"),
            onClick: () => {
              ctx.addBlockedFolder(projectPath);
              ctx.onRefreshTree?.();
            },
          },
        ]
      : []),
    { label: "", separator: true, onClick: () => {} },
    {
      label: t("contextMenu.deleteAll"),
      onClick: () => ctx.trashAllUnderNode(node),
    },
  ];
}
