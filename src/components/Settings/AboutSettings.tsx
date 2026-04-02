import { createSignal, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "../../i18n/index";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export function AboutSettings() {
  const { t } = useI18n();
  const [version, setVersion] = createSignal("0.1.0");
  const [updateChecking, setUpdateChecking] = createSignal(false);
  const [updateStatus, setUpdateStatus] = createSignal<string | null>(null);

  onMount(async () => {
    try {
      const { getVersion } = await import("@tauri-apps/api/app");
      setVersion(await getVersion());
    } catch {
      /* fallback */
    }
  });

  async function handleCheckUpdate() {
    setUpdateChecking(true);
    setUpdateStatus(null);
    try {
      const update = await check();
      if (update) {
        const yes = confirm(
          `${t("settings.updateAvailable")}: v${update.version}\n\n${t("settings.updateConfirm")}`,
        );
        if (yes) {
          setUpdateStatus(t("settings.updating"));
          await update.downloadAndInstall();
          await relaunch();
        } else {
          setUpdateStatus(`v${update.version} ${t("settings.updateReady")}`);
        }
      } else {
        setUpdateStatus(t("settings.upToDate"));
        setTimeout(() => setUpdateStatus(null), 3000);
      }
    } catch (e) {
      setUpdateStatus(t("settings.updateFailed"));
      console.warn("Update check failed:", e);
      setTimeout(() => setUpdateStatus(null), 3000);
    } finally {
      setUpdateChecking(false);
    }
  }

  return (
    <div class="settings-section">
      <div class="settings-section-title">{t("settings.about")}</div>

      <div class="settings-row">
        <div class="settings-label">{t("settings.version")}</div>
        <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
          <span class="settings-stat">{version()}</span>
          <button
            class="settings-btn"
            disabled={updateChecking()}
            onClick={handleCheckUpdate}
          >
            {updateChecking()
              ? "..."
              : updateStatus() || t("settings.checkUpdate")}
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
            invoke("open_external", {
              url: "https://github.com/tyql688/cc-session",
            }).catch((e) => console.error("Failed to open GitHub:", e));
          }}
        >
          cc-session
        </a>
      </div>
    </div>
  );
}
