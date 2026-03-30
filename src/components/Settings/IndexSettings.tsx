import { createSignal, createResource } from "solid-js";
import { useI18n } from "../../i18n/index";
import type { IndexStats } from "../../lib/types";
import { getIndexStats, rebuildIndex, clearIndex } from "../../lib/tauri";
import { toast, toastError } from "../../stores/toast";
import { errorMessage } from "../../lib/errors";
import { formatFileSize } from "../../lib/formatters";

export function IndexSettings(props: { onIndexChanged: () => void }) {
  const { t } = useI18n();
  const [isRebuilding, setIsRebuilding] = createSignal(false);

  const [indexStats, { refetch: refetchStats }] = createResource<IndexStats>(
    async () => {
      try {
        return await getIndexStats();
      } catch {
        return { session_count: 0, db_size_bytes: 0, last_index_time: "" };
      }
    },
  );

  async function handleRebuildIndex() {
    setIsRebuilding(true);
    try {
      await rebuildIndex();
      refetchStats();
      props.onIndexChanged();
      toast(t("toast.rebuildOk"));
    } catch (_e) {
      toastError(t("toast.rebuildFailed"));
    } finally {
      setIsRebuilding(false);
    }
  }

  return (
    <div class="settings-section">
      <div class="settings-section-title">{t("settings.index")}</div>

      <div class="settings-row">
        <div class="settings-label">{t("settings.totalSessions")}</div>
        <span class="settings-stat">{indexStats()?.session_count ?? 0}</span>
      </div>

      <div class="settings-row">
        <div class="settings-label">{t("settings.dbSize")}</div>
        <span class="settings-stat">
          {formatFileSize(indexStats()?.db_size_bytes ?? 0)}
        </span>
      </div>

      <div class="settings-row settings-row-spaced">
        <button
          class="settings-btn"
          onClick={handleRebuildIndex}
          disabled={isRebuilding()}
        >
          {isRebuilding() ? "..." : t("settings.rebuildIndex")}
        </button>
        <button
          class="settings-btn settings-btn-danger"
          onClick={async () => {
            if (!confirm(t("settings.clearIndexConfirm"))) return;
            try {
              await clearIndex();
              toast(t("toast.clearIndexOk"));
              refetchStats();
              props.onIndexChanged();
            } catch (e) {
              toastError(errorMessage(e));
            }
          }}
        >
          {t("settings.clearIndex")}
        </button>
      </div>
    </div>
  );
}
