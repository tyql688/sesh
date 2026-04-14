import { createSignal, For } from "solid-js";
import type { SessionRef, Provider } from "../lib/types";
import { useI18n } from "../i18n/index";
import { ContextMenu, type MenuItemDef } from "./ContextMenu";
import { isMac } from "../lib/platform";
import { moveTabToGroup } from "../stores/editorGroups";

function providerColor(provider: Provider): string {
  return `var(--${provider})`;
}

export function TabBar(props: {
  groupId: string;
  tabs: SessionRef[];
  activeTabId: string | null;
  onTabSelect: (id: string) => void;
  onTabClose: (id: string) => void;
  onCloseAllTabs: () => void;
  onCloseOtherTabs: (keepId: string) => void;
  onCloseTabsToRight: (fromId: string) => void;
  onSplitToRight: (sessionId: string) => void;
}) {
  const { t } = useI18n();
  const [menuState, setMenuState] = createSignal<{
    pos: { x: number; y: number };
    tabId: string;
  } | null>(null);

  function handleContextMenu(e: MouseEvent, tabId: string) {
    e.preventDefault();
    e.stopPropagation();
    setMenuState({ pos: { x: e.clientX, y: e.clientY }, tabId });
  }

  function menuItems(): MenuItemDef[] {
    const m = menuState();
    if (!m) return [];
    return [
      {
        label: t("contextMenu.close"),
        shortcut: isMac ? "\u2318W" : "Ctrl+W",
        onClick: () => props.onTabClose(m.tabId),
      },
      {
        label: t("contextMenu.closeOthers"),
        onClick: () => props.onCloseOtherTabs(m.tabId),
      },
      {
        label: t("contextMenu.closeToRight"),
        onClick: () => props.onCloseTabsToRight(m.tabId),
      },
      {
        label: t("contextMenu.openToSide"),
        onClick: () => props.onSplitToRight(m.tabId),
      },
      { label: "", separator: true, onClick: () => {} },
      {
        label: t("contextMenu.closeAll"),
        shortcut: isMac ? "\u21E7\u2318W" : "Ctrl+Shift+W",
        onClick: () => props.onCloseAllTabs(),
      },
    ];
  }

  return (
    <div
      class="tab-bar"
      onDragOver={(e) => {
        e.preventDefault();
        if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
      }}
      onDrop={(e) => {
        e.preventDefault();
        try {
          const data = JSON.parse(
            e.dataTransfer?.getData("text/plain") ?? "{}",
          );
          if (data.sessionId && data.sourceGroupId !== props.groupId) {
            moveTabToGroup(data.sessionId, props.groupId);
          }
        } catch {
          /* ignore */
        }
      }}
    >
      <For each={props.tabs}>
        {(tab) => {
          const isActive = () => tab.id === props.activeTabId;
          return (
            <div
              class={`tab${isActive() ? " active" : ""}`}
              draggable={true}
              onDragStart={(e) => {
                e.dataTransfer!.setData(
                  "text/plain",
                  JSON.stringify({
                    sessionId: tab.id,
                    sourceGroupId: props.groupId,
                  }),
                );
                e.dataTransfer!.effectAllowed = "move";
                (e.currentTarget as HTMLElement).style.opacity = "0.4";
              }}
              onDragEnd={(e) => {
                (e.currentTarget as HTMLElement).style.opacity = "";
              }}
              onClick={(e) => {
                if (e.button === 0) props.onTabSelect(tab.id);
              }}
              onMouseDown={(e) => {
                if (e.button === 1) {
                  e.preventDefault();
                  props.onTabClose(tab.id);
                }
              }}
              onContextMenu={(e) => handleContextMenu(e, tab.id)}
            >
              <span
                class="tab-dot"
                style={{ background: providerColor(tab.provider) }}
              />
              <span class="tab-title">{tab.title}</span>
              <button
                class={`tab-close${isActive() ? " visible" : ""}`}
                aria-label={t("common.closeTab")}
                onClick={(e) => {
                  e.stopPropagation();
                  props.onTabClose(tab.id);
                }}
              >
                &times;
              </button>
            </div>
          );
        }}
      </For>

      <ContextMenu
        items={menuItems()}
        position={menuState()?.pos ?? null}
        onClose={() => setMenuState(null)}
      />
    </div>
  );
}
