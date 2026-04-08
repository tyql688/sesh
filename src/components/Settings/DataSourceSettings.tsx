import { For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "../../i18n/index";
import type { ProviderSnapshot } from "../../lib/types";
import { disabledProviders, toggleProvider } from "../../stores/settings";
import { toastError } from "../../stores/toast";

export function DataSourceSettings(props: {
  providerSnapshots: () => ProviderSnapshot[];
}) {
  const { t } = useI18n();

  return (
    <div class="settings-section">
      <div class="settings-section-title">{t("settings.dataSources")}</div>
      <For each={props.providerSnapshots()}>
        {(info) => (
          <div class="settings-row">
            <div>
              <div class="settings-label">{info.label}</div>
              <div class="settings-desc flex-center-gap-sm">
                <span>{info.path}</span>
                <Show when={info.exists}>
                  <button
                    class="settings-open-folder"
                    title={t("settings.openInFinder")}
                    onClick={async () => {
                      try {
                        await invoke("open_external", { url: info.path });
                      } catch (e) {
                        toastError(String(e));
                      }
                    }}
                  >
                    <svg
                      width="12"
                      height="12"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                    >
                      <path d="M18 13v6a2 2 0 01-2 2H5a2 2 0 01-2-2V8a2 2 0 012-2h6" />
                      <polyline points="15 3 21 3 21 9" />
                      <line x1="10" y1="14" x2="21" y2="3" />
                    </svg>
                  </button>
                </Show>
              </div>
            </div>
            <div class="flex-center-gap-md">
              <span class="settings-stat">
                {info.session_count} {t("status.sessions")}
              </span>
              <Show when={info.exists}>
                <button
                  class={`settings-btn${disabledProviders().includes(info.key) ? " settings-btn-danger" : ""}`}
                  onClick={() => toggleProvider(info.key)}
                >
                  {disabledProviders().includes(info.key)
                    ? t("settings.disabled")
                    : t("settings.enabled")}
                </button>
              </Show>
              <Show when={!info.exists}>
                <span class="settings-stat text-danger">
                  {t("settings.disabled")}
                </span>
              </Show>
            </div>
          </div>
        )}
      </For>
    </div>
  );
}
