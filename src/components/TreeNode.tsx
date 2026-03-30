import { For, Show } from "solid-js";
import type { TreeNode } from "../lib/types";
import { useI18n } from "../i18n/index";
import { isSelected } from "../stores/selection";
import { ProviderDot } from "../lib/icons";

// Re-exports for backward compatibility
export { ProviderDot } from "../lib/icons";
export { collectSessionIds, collectSessionNodes } from "../lib/tree-utils";

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

export function formatSessionLabel(raw: string, fallback = "Untitled"): string {
  let label = raw;
  label = label.replace(/^##\s*TASK:\s*/i, "");
  label = label.replace(/^\d+\.\s*TASK:\s*/i, "");
  label = label.replace(/^\[search-mode\]\s*/i, "");
  label = label.replace(/^CONTEXT:\s*/i, "");
  label = label.replace(/^TASK:\s*/i, "");
  label = label.trim();

  if (/^[/~.]/.test(label) && label.includes("/")) {
    const segments = label.split("/").filter(Boolean);
    if (segments.length > 0) {
      label = segments[segments.length - 1];
    }
  }

  if (label.length > 40) {
    label = label.slice(0, 37) + "...";
  }

  return label || fallback;
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
    parentProjectLabel: string,
  ) => void;
  onNodeContextMenu: (e: MouseEvent, node: TreeNode) => void;
  onSessionClick: (
    e: MouseEvent,
    node: TreeNode,
    parentProjectLabel: string,
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
      props.onSessionContextMenu(e, props.node, props.parentProjectLabel ?? "");
    } else {
      props.onNodeContextMenu(e, props.node);
    }
  };

  const projectLabel = () =>
    props.node.node_type === "project"
      ? props.node.label === "(No Project)"
        ? t("explorer.noProject")
        : props.node.label
      : props.parentProjectLabel;

  const displayLabel = () => {
    if (
      props.node.node_type === "project" &&
      props.node.label === "(No Project)"
    ) {
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
            ? formatSessionLabel(props.node.label, t("common.untitled"))
            : displayLabel()}
        </span>

        <Show when={props.node.is_sidechain}>
          <span class="tree-node-sidechain" title={t("common.subagentSession")}>
            ⤷
          </span>
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
