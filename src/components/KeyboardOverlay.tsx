import { For, Show } from "solid-js";
import { useI18n } from "../i18n/index";

const isMac = navigator.platform.includes("Mac");
const mod = isMac ? "\u2318" : "Ctrl+";
const shift = isMac ? "\u21E7" : "Shift+";

interface ShortcutItem {
  keys: string;
  descKey: string;
}

interface ShortcutCategory {
  categoryKey: string;
  items: ShortcutItem[];
}

const shortcuts: ShortcutCategory[] = [
  {
    categoryKey: "keyboard.navigation",
    items: [
      { keys: `${shift}${mod}F`, descKey: "keyboard.globalSearch" },
      { keys: `${mod}1-9`, descKey: "keyboard.switchTab" },
      { keys: isMac ? `${mod}]` : "Ctrl+Tab", descKey: "keyboard.nextTab" },
      { keys: isMac ? `${mod}[` : `${shift}Ctrl+Tab`, descKey: "keyboard.prevTab" },
    ],
  },
  {
    categoryKey: "keyboard.tabs",
    items: [
      { keys: `${mod}W`, descKey: "keyboard.closeTab" },
      { keys: `${shift}${mod}W`, descKey: "keyboard.closeAllTabs" },
    ],
  },
  {
    categoryKey: "keyboard.session",
    items: [
      { keys: `${mod}F`, descKey: "keyboard.findInSession" },
      { keys: `${shift}${mod}R`, descKey: "keyboard.resumeSession" },
      { keys: `${shift}${mod}E`, descKey: "keyboard.exportSession" },
      { keys: `${mod}B`, descKey: "keyboard.toggleFavorite" },
      { keys: `${mod}L`, descKey: "keyboard.toggleWatch" },
      { keys: `${mod}\u232B`, descKey: "keyboard.deleteSession" },
    ],
  },
  {
    categoryKey: "keyboard.general",
    items: [
      { keys: `${mod},`, descKey: "keyboard.openSettings" },
      { keys: `${mod}R`, descKey: "keyboard.refresh" },
      { keys: `${mod}/`, descKey: "keyboard.showShortcuts" },
      { keys: "?", descKey: "keyboard.showShortcuts" },
      { keys: "Esc", descKey: "keyboard.escape" },
    ],
  },
];

export function KeyboardOverlay(props: { show: boolean; onClose: () => void }) {
  const { t } = useI18n();

  return (
    <Show when={props.show}>
      <div class="keyboard-overlay-backdrop" onClick={() => props.onClose()}>
        <div class="keyboard-overlay" role="dialog" aria-modal="true" aria-label={t("keyboard.title")} onClick={(e) => e.stopPropagation()}>
          <div class="keyboard-overlay-header">
            <span class="keyboard-overlay-title">{t("keyboard.title")}</span>
            <button class="keyboard-overlay-close" onClick={() => props.onClose()}>
              <svg width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </div>
          <div class="keyboard-grid">
            <For each={shortcuts}>
              {(cat) => (
                <div>
                  <div class="keyboard-category-title">{t(cat.categoryKey)}</div>
                  <For each={cat.items}>
                    {(item) => (
                      <div class="keyboard-item">
                        <span class="keyboard-item-desc">{t(item.descKey) || item.descKey}</span>
                        <span class="keyboard-keys">{item.keys}</span>
                      </div>
                    )}
                  </For>
                </div>
              )}
            </For>
          </div>
        </div>
      </div>
    </Show>
  );
}
