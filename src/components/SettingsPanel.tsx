import { createSignal, For, Show } from "solid-js";
import { useI18n } from "../i18n/index";
import { GeneralSettings } from "./Settings/GeneralSettings";
import { DataSourceSettings } from "./Settings/DataSourceSettings";
import { IndexSettings } from "./Settings/IndexSettings";
import { KeyboardSettings } from "./Settings/KeyboardSettings";
import { AboutSettings } from "./Settings/AboutSettings";
import {
  listLoadedProviderSnapshots,
  refreshProviderSnapshots,
} from "../stores/providerSnapshots";

type SettingsCategory =
  | "general"
  | "dataSources"
  | "index"
  | "keyboard"
  | "about";

export function SettingsPanel() {
  const { t } = useI18n();
  const [activeCategory, setActiveCategory] =
    createSignal<SettingsCategory>("general");

  const categories = [
    {
      id: "general" as SettingsCategory,
      labelKey: "settings.general" as const,
    },
    {
      id: "dataSources" as SettingsCategory,
      labelKey: "settings.dataSources" as const,
    },
    { id: "index" as SettingsCategory, labelKey: "settings.index" as const },
    { id: "keyboard" as SettingsCategory, labelKey: "keyboard.title" as const },
    { id: "about" as SettingsCategory, labelKey: "settings.about" as const },
  ];

  function handleIndexChanged() {
    void refreshProviderSnapshots();
  }

  return (
    <div class="settings-panel">
      <div class="settings-sidebar">
        <For each={categories}>
          {(cat) => (
            <button
              class={`settings-nav-item${activeCategory() === cat.id ? " active" : ""}`}
              onClick={() => setActiveCategory(cat.id)}
            >
              {t(cat.labelKey)}
            </button>
          )}
        </For>
      </div>

      <div class="settings-content">
        <Show when={activeCategory() === "general"}>
          <GeneralSettings />
        </Show>

        <Show when={activeCategory() === "dataSources"}>
          <DataSourceSettings providerSnapshots={listLoadedProviderSnapshots} />
        </Show>

        <Show when={activeCategory() === "index"}>
          <IndexSettings onIndexChanged={handleIndexChanged} />
        </Show>

        <Show when={activeCategory() === "keyboard"}>
          <KeyboardSettings />
        </Show>

        <Show when={activeCategory() === "about"}>
          <AboutSettings />
        </Show>
      </div>
    </div>
  );
}
