import { For, Show } from "solid-js";
import type { TreeNode, Provider } from "../lib/types";
import { useI18n } from "../i18n/index";
import { isSelected } from "../stores/selection";

export function ChevronIcon(props: { expanded: boolean }) {
  return (
    <svg
      width="16"
      height="16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      viewBox="0 0 24 24"
      class={`chevron${props.expanded ? " expanded" : ""}`}
    >
      <polyline points="9 18 15 12 9 6" />
    </svg>
  );
}

export function FolderIcon() {
  return (
    <svg
      width="16"
      height="16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      viewBox="0 0 24 24"
    >
      <path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z" />
    </svg>
  );
}

export function ChatIcon() {
  return (
    <svg
      width="16"
      height="16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      viewBox="0 0 24 24"
    >
      <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
    </svg>
  );
}

export function ProviderDot(props: { provider: Provider }) {
  const color = () => {
    switch (props.provider) {
      case "claude": return "var(--claude)";
      case "codex": return "var(--codex)";
      case "gemini": return "var(--gemini)";
      case "cursor": return "var(--cursor)";
      case "opencode": return "var(--opencode)";
    }
  };
  return (
    <span class="provider-dot provider-logo" style={{ color: color() }}>
      {props.provider === "claude" && (
        <svg width="12" height="12" fill="currentColor" viewBox="0 0 24 24">
          <path d="M17.3041 3.541h-3.6718l6.696 16.918H24Zm-10.6082 0L0 20.459h3.7442l1.3693-3.5527h7.0052l1.3693 3.5528h3.7442L10.5363 3.5409Zm-.3712 10.2232 2.2914-5.9456 2.2914 5.9456Z" />
        </svg>
      )}
      {props.provider === "codex" && (
        <svg width="12" height="12" fill="currentColor" viewBox="0 0 24 24">
          <path d="M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z" />
        </svg>
      )}
      {props.provider === "gemini" && (
        <svg width="12" height="12" fill="currentColor" viewBox="0 0 24 24">
          <path d="M11.04 19.32Q12 21.51 12 24q0-2.49.93-4.68.96-2.19 2.58-3.81t3.81-2.55Q21.51 12 24 12q-2.49 0-4.68-.93a12.3 12.3 0 0 1-3.81-2.58 12.3 12.3 0 0 1-2.58-3.81Q12 2.49 12 0q0 2.49-.96 4.68-.93 2.19-2.55 3.81a12.3 12.3 0 0 1-3.81 2.58Q2.49 12 0 12q2.49 0 4.68.96 2.19.93 3.81 2.55t2.55 3.81" />
        </svg>
      )}
      {props.provider === "cursor" && (
        <svg width="12" height="12" fill="currentColor" viewBox="0 0 24 24">
          <path d="M11.503.131 1.891 5.678a.84.84 0 0 0-.42.726v11.188c0 .3.162.575.42.724l9.609 5.55a1 1 0 0 0 .998 0l9.61-5.55a.84.84 0 0 0 .42-.724V6.404a.84.84 0 0 0-.42-.726L12.497.131a1.01 1.01 0 0 0-.996 0M2.657 6.338h18.55c.263 0 .43.287.297.515L12.23 22.918c-.062.107-.229.064-.229-.06V12.335a.59.59 0 0 0-.295-.51l-9.11-5.257c-.109-.063-.064-.23.061-.23" />
        </svg>
      )}
      {props.provider === "opencode" && (
        <svg width="12" height="12" fill="currentColor" viewBox="0 0 24 24">
          <path fill-rule="evenodd" clip-rule="evenodd" d="M18 19.5H6V4.5H18V19.5ZM15 7.5H9V16.5H15V7.5Z" />
        </svg>
      )}
    </span>
  );
}

export function formatSessionLabel(raw: string): string {
  let label = raw;
  label = label.replace(/^##\s*TASK:\s*/i, "");
  label = label.replace(/^\d+\.\s*TASK:\s*/i, "");
  label = label.replace(/^\[search-mode\]\s*/i, "");
  label = label.replace(/^CONTEXT:\s*/i, "");
  label = label.replace(/^TASK:\s*/i, "");
  label = label.trim();

  if (/^[\/~.]/.test(label) && label.includes("/")) {
    const segments = label.split("/").filter(Boolean);
    if (segments.length > 0) {
      label = segments[segments.length - 1];
    }
  }

  if (label.length > 40) {
    label = label.slice(0, 37) + "...";
  }

  return label || "Untitled";
}

/** Collect all session-leaf IDs from a tree node recursively. */
export function collectSessionIds(node: TreeNode): string[] {
  if (node.node_type === "session") {
    return [node.id];
  }
  const ids: string[] = [];
  for (const child of node.children) {
    ids.push(...collectSessionIds(child));
  }
  return ids;
}

/** Collect session nodes (with metadata) from a tree node. */
export function collectSessionNodes(node: TreeNode): TreeNode[] {
  if (node.node_type === "session") {
    return [node];
  }
  const nodes: TreeNode[] = [];
  for (const child of node.children) {
    nodes.push(...collectSessionNodes(child));
  }
  return nodes;
}

export function TreeNodeComponent(props: {
  node: TreeNode;
  depth: number;
  activeSessionId: string | null;
  parentProjectLabel?: string;
  isNodeExpanded: (nodeId: string) => boolean;
  toggleExpanded: (nodeId: string) => void;
  onSessionContextMenu: (
    e: MouseEvent,
    node: TreeNode,
    parentProjectLabel: string
  ) => void;
  onNodeContextMenu: (e: MouseEvent, node: TreeNode) => void;
  onSessionClick: (
    e: MouseEvent,
    node: TreeNode,
    parentProjectLabel: string
  ) => void;
}) {
  const { t } = useI18n();
  const isLeaf = () => props.node.node_type === "session";
  const expanded = () => props.isNodeExpanded(props.node.id);

  const handleClick = (e: MouseEvent) => {
    if (isLeaf()) {
      props.onSessionClick(e, props.node, props.parentProjectLabel ?? "");
    } else {
      props.toggleExpanded(props.node.id);
    }
  };

  const handleContextMenu = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (isLeaf()) {
      props.onSessionContextMenu(
        e,
        props.node,
        props.parentProjectLabel ?? ""
      );
    } else {
      props.onNodeContextMenu(e, props.node);
    }
  };

  const projectLabel = () =>
    props.node.node_type === "project"
      ? props.node.label === "(No Project)" ? t("explorer.noProject") : props.node.label
      : props.parentProjectLabel;

  const displayLabel = () => {
    if (props.node.node_type === "project" && props.node.label === "(No Project)") {
      return t("explorer.noProject");
    }
    return props.node.label;
  };

  const nodeSelected = () => isLeaf() && isSelected(props.node.id);

  return (
    <div class="tree-node-wrapper">
      <button
        class={`tree-node tree-node-${props.node.node_type}${isLeaf() && props.activeSessionId === props.node.id ? " active" : ""}${nodeSelected() ? " selected" : ""}`}
        style={{ "padding-left": `${props.depth * 16 + 8}px` }}
        onClick={handleClick}
        onContextMenu={handleContextMenu}
        data-session-id={isLeaf() ? props.node.id : undefined}
      >
        <Show when={!isLeaf()}>
          <ChevronIcon expanded={expanded()} />
        </Show>
        <Show when={isLeaf()}>
          <span class="tree-node-icon-spacer" />
        </Show>

        <Show when={props.node.node_type === "provider" && props.node.provider}>
          <ProviderDot provider={props.node.provider!} />
        </Show>
        <Show when={props.node.node_type === "project"}>
          <span class="tree-node-icon">
            <FolderIcon />
          </span>
        </Show>
        <Show when={props.node.node_type === "session"}>
          <span class="tree-node-icon">
            <ChatIcon />
          </span>
        </Show>

        <span
          class={`tree-node-label${props.node.node_type === "provider" ? " bold" : ""}`}
          title={
            props.node.node_type === "session" ? props.node.label : undefined
          }
        >
          {props.node.node_type === "session"
            ? formatSessionLabel(props.node.label)
            : displayLabel()}
        </span>

        <Show when={props.node.is_sidechain}>
          <span class="tree-node-sidechain" title="Subagent session">⤷</span>
        </Show>
        <Show when={props.node.count > 0 && !isLeaf()}>
          <span class="tree-node-count">{props.node.count}</span>
        </Show>
      </button>

      <Show when={expanded() && !isLeaf()}>
        <For each={props.node.children}>
          {(child) => (
            <TreeNodeComponent
              node={child}
              depth={props.depth + 1}
              activeSessionId={props.activeSessionId}
              parentProjectLabel={projectLabel()}
              isNodeExpanded={props.isNodeExpanded}
              toggleExpanded={props.toggleExpanded}
              onSessionContextMenu={props.onSessionContextMenu}
              onNodeContextMenu={props.onNodeContextMenu}
              onSessionClick={props.onSessionClick}
            />
          )}
        </For>
      </Show>
    </div>
  );
}
