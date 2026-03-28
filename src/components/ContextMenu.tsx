import { For, Show, onMount, onCleanup } from "solid-js";

export interface MenuItemDef {
  label: string | (() => string);
  shortcut?: string;
  danger?: boolean;
  separator?: boolean;
  onClick: () => void;
}

interface Props {
  items: MenuItemDef[];
  position: { x: number; y: number } | null;
  onClose: () => void;
}

export function ContextMenu(props: Props) {
  function handleDocClick() {
    props.onClose();
  }

  onMount(() => document.addEventListener("click", handleDocClick));
  onCleanup(() => document.removeEventListener("click", handleDocClick));

  return (
    <Show when={props.position}>
      {(pos) => (
        <div
          class="context-menu"
          ref={(el) => {
            requestAnimationFrame(() => {
              const rect = el.getBoundingClientRect();
              const vw = window.innerWidth;
              const vh = window.innerHeight;
              if (rect.right > vw) el.style.left = `${Math.max(4, pos().x - rect.width)}px`;
              if (rect.bottom > vh) el.style.top = `${Math.max(4, pos().y - rect.height)}px`;
            });
          }}
          style={{ left: `${pos().x}px`, top: `${pos().y}px` }}
          onClick={(e) => e.stopPropagation()}
        >
          <For each={props.items}>
            {(item) => (
              <Show
                when={!item.separator}
                fallback={<div class="context-menu-separator" />}
              >
                <button
                  class={`context-menu-item${item.danger ? " danger" : ""}`}
                  onClick={() => {
                    item.onClick();
                    props.onClose();
                  }}
                >
                  <span>
                    {typeof item.label === "function"
                      ? item.label()
                      : item.label}
                  </span>
                  <Show when={item.shortcut}>
                    <span class="shortcut">{item.shortcut}</span>
                  </Show>
                </button>
              </Show>
            )}
          </For>
        </div>
      )}
    </Show>
  );
}
