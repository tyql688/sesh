import { createSignal, Show, For } from "solid-js";
import { save } from "@tauri-apps/plugin-dialog";
import type { SessionMeta } from "../lib/types";
import { exportSession } from "../lib/tauri";
import { useI18n } from "../i18n/index";
import { toast, toastError } from "../stores/toast";
import { errorMessage } from "../lib/errors";

type ExportFormat = "json" | "markdown" | "html";

const FORMAT_OPTIONS: { value: ExportFormat; labelKey: string; ext: string }[] =
  [
    { value: "json", labelKey: "export.json", ext: "json" },
    { value: "markdown", labelKey: "export.markdown", ext: "md" },
    { value: "html", labelKey: "export.html", ext: "html" },
  ];

export function ExportDialog(props: {
  open: boolean;
  session: SessionMeta;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [format, setFormat] = createSignal<ExportFormat>("json");
  const [exporting, setExporting] = createSignal(false);

  function handleOverlayClick(e: MouseEvent) {
    if (e.target === e.currentTarget) {
      props.onClose();
    }
  }

  async function handleExport() {
    const selected = FORMAT_OPTIONS.find((f) => f.value === format());
    if (!selected) return;

    try {
      const outputPath = await save({
        defaultPath: `${props.session.title || "session"}.${selected.ext}`,
        filters: [
          { name: selected.value.toUpperCase(), extensions: [selected.ext] },
        ],
      });

      if (!outputPath) return;

      setExporting(true);
      await exportSession(
        props.session.id,
        props.session.source_path,
        props.session.provider,
        selected.value,
        outputPath,
      );
      props.onClose();
      toast(t("toast.exportOk"));
    } catch (e) {
      toastError(errorMessage(e));
    } finally {
      setExporting(false);
    }
  }

  return (
    <Show when={props.open}>
      <div class="modal-overlay" onClick={handleOverlayClick}>
        <div class="modal-card">
          <div class="modal-title">{t("export.title")}</div>
          <div class="export-formats">
            <For each={FORMAT_OPTIONS}>
              {(opt) => (
                <button
                  class={`export-format-card ${format() === opt.value ? "active" : ""}`}
                  onClick={() => setFormat(opt.value)}
                >
                  <span class="export-format-label">{t(opt.labelKey)}</span>
                  <span class="export-format-ext">.{opt.ext}</span>
                </button>
              )}
            </For>
          </div>
          <div class="modal-actions">
            <button class="btn btn-secondary" onClick={props.onClose}>
              {t("confirm.cancel")}
            </button>
            <button
              class="btn btn-primary"
              onClick={handleExport}
              disabled={exporting()}
            >
              {exporting() ? "..." : t("session.export")}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}
