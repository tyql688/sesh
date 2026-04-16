import { onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { createSignal } from "solid-js";
import { useI18n } from "../../i18n/index";
import { errorMessage } from "../../lib/errors";
import { invokeWithToast } from "../../lib/tauri";
import {
  phase,
  availableVersion,
  errorDetail,
  checkForUpdate,
  downloadAndInstall,
} from "../../stores/updater";

export function AboutSettings() {
  const { t } = useI18n();
  const [version, setVersion] = createSignal<string | null>(null);
  const [versionError, setVersionError] = createSignal<string | null>(null);

  onMount(async () => {
    try {
      const { getVersion } = await import("@tauri-apps/api/app");
      setVersion(await getVersion());
      setVersionError(null);
    } catch (error) {
      console.error("Failed to load app version:", error);
      setVersion(null);
      setVersionError(errorMessage(error));
    }
  });

  const buttonLabel = () => {
    switch (phase()) {
      case "checking":
        return "...";
      case "upToDate":
        return t("settings.upToDate");
      case "available":
        return `↑ v${availableVersion()}`;
      case "downloading":
      case "installing":
        return t("settings.updating");
      case "error":
        return t("settings.updateFailed");
      default:
        return t("settings.checkUpdate");
    }
  };

  const isDisabled = () =>
    phase() === "checking" ||
    phase() === "upToDate" ||
    phase() === "downloading" ||
    phase() === "installing";

  function handleClick() {
    if (phase() === "available") {
      void downloadAndInstall();
    } else if (phase() === "idle" || phase() === "error") {
      void checkForUpdate();
    }
  }

  return (
    <div class="settings-section">
      <div class="settings-section-title">{t("settings.about")}</div>

      <div class="settings-row">
        <div class="settings-label">{t("settings.version")}</div>
        <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
          <span class="settings-stat" title={versionError() ?? undefined}>
            {version() ?? "—"}
          </span>
          <button
            class="settings-btn"
            disabled={isDisabled()}
            onClick={handleClick}
            title={phase() === "error" ? (errorDetail() ?? "") : ""}
          >
            {buttonLabel()}
          </button>
        </div>
      </div>

      <div class="settings-row">
        <div class="settings-label">{t("settings.github")}</div>
        <a
          class="settings-stat link-accent"
          href="https://github.com/tyql688/cc-session"
          onClick={(e) => {
            e.preventDefault();
            void invokeWithToast(
              invoke<void>("open_external", {
                url: "https://github.com/tyql688/cc-session",
              }),
              "open GitHub link",
            );
          }}
        >
          cc-session
        </a>
      </div>
    </div>
  );
}
