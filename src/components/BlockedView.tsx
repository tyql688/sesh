import { For, Show } from "solid-js";
import { useI18n } from "../i18n/index";
import { blockedFolders, removeBlockedFolder } from "../stores/settings";

export function BlockedView(props: { onRefreshTree?: () => void }) {
  const { t } = useI18n();

  return (
    <div class="blocked-view">
      <div class="explorer-header">{t("settings.blockedFolders")}</div>
      <Show
        when={blockedFolders().length > 0}
        fallback={
          <div class="empty-state">
            <svg width="32" height="32" fill="none" stroke="currentColor" stroke-width="1.5" viewBox="0 0 24 24" style="opacity:0.4">
              <circle cx="12" cy="12" r="10" />
              <line x1="4.93" y1="4.93" x2="19.07" y2="19.07" />
            </svg>
            <p class="empty-state-text">{t("settings.noBlockedFolders")}</p>
            <p class="empty-state-hint">{t("blocked.hint")}</p>
          </div>
        }
      >
        <div class="blocked-list">
          <For each={blockedFolders()}>
            {(folder) => (
              <div class="blocked-item">
                <div class="blocked-item-info">
                  <span class="blocked-item-name">{folder.split("/").pop() || folder}</span>
                  <span class="blocked-item-path" title={folder}>{folder}</span>
                </div>
                <button
                  class="blocked-item-btn"
                  title={t("settings.unblock")}
                  onClick={() => {
                    removeBlockedFolder(folder);
                    props.onRefreshTree?.();
                  }}
                >
                  <svg width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" viewBox="0 0 24 24">
                    <line x1="18" y1="6" x2="6" y2="18" />
                    <line x1="6" y1="6" x2="18" y2="18" />
                  </svg>
                </button>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}
