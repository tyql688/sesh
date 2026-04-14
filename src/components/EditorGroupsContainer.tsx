import { For, Show, createSignal } from "solid-js";
import type { SessionRef, TreeNode } from "../lib/types";
import {
  groups,
  activeGroupId,
  focusGroup,
  setGroupFlexBasis,
  createGroupFromDrop,
} from "../stores/editorGroups";
import { EditorArea } from "./EditorArea";
import { SplitHandle } from "./SplitHandle";

export function EditorGroupsContainer(props: {
  onTabSelect: (groupId: string, tabId: string) => void;
  onTabClose: (sessionId: string) => void;
  onCloseAllTabs: () => void;
  onCloseOtherTabs: (keepId: string) => void;
  onCloseTabsToRight: (fromId: string) => void;
  onSplitToRight: (sessionId: string) => void;
  onRefreshTree: () => void;
  tree: TreeNode[];
  onOpenSession: (session: SessionRef) => void;
}) {
  const [dropActive, setDropActive] = createSignal(false);

  function handleResize(leftIdx: number, deltaX: number) {
    const gs = groups();
    const left = gs[leftIdx];
    const right = gs[leftIdx + 1];
    if (!left || !right) return;

    const container = document.querySelector(
      ".editor-groups-container",
    ) as HTMLElement;
    if (!container) return;
    const totalWidth = container.clientWidth;
    const deltaPct = (deltaX / totalWidth) * 100;

    const newLeft = Math.max(15, left.flexBasis + deltaPct);
    const newRight = Math.max(15, right.flexBasis - deltaPct);
    setGroupFlexBasis(left.id, newLeft);
    setGroupFlexBasis(right.id, newRight);
  }

  function equalizeWidths() {
    const gs = groups();
    const basis = 100 / gs.length;
    for (const g of gs) {
      setGroupFlexBasis(g.id, basis);
    }
  }

  function handleDragOver(e: DragEvent) {
    const container = e.currentTarget as HTMLElement;
    const rect = container.getBoundingClientRect();
    const inDropZone = e.clientX > rect.right - 40;
    setDropActive(inDropZone);
    if (inDropZone) {
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    }
  }

  function handleDrop(e: DragEvent) {
    if (!dropActive()) return;
    e.preventDefault();
    setDropActive(false);
    try {
      const data = JSON.parse(e.dataTransfer?.getData("text/plain") ?? "{}");
      if (data.sessionId) {
        createGroupFromDrop(data.sessionId);
      }
    } catch {
      /* ignore invalid drag data */
    }
  }

  function handleDragLeave() {
    setDropActive(false);
  }

  return (
    <div
      class="editor-groups-container"
      onDragOver={handleDragOver}
      onDrop={handleDrop}
      onDragLeave={handleDragLeave}
    >
      <For each={groups()}>
        {(group, idx) => (
          <>
            <Show when={idx() > 0}>
              <SplitHandle
                onResize={(dx) => handleResize(idx() - 1, dx)}
                onDoubleClick={equalizeWidths}
              />
            </Show>
            <EditorArea
              groupId={group.id}
              tabs={group.tabs}
              activeTabId={group.activeTabId}
              isFocused={group.id === activeGroupId()}
              flexBasis={group.flexBasis}
              onFocus={() => focusGroup(group.id)}
              onTabSelect={(tabId) => props.onTabSelect(group.id, tabId)}
              onTabClose={props.onTabClose}
              onCloseAllTabs={props.onCloseAllTabs}
              onCloseOtherTabs={props.onCloseOtherTabs}
              onCloseTabsToRight={props.onCloseTabsToRight}
              onSplitToRight={props.onSplitToRight}
              onRefreshTree={props.onRefreshTree}
              tree={props.tree}
              onOpenSession={props.onOpenSession}
            />
          </>
        )}
      </For>
      <div class={`editor-groups-drop-right${dropActive() ? " active" : ""}`} />
    </div>
  );
}
