import { Show } from "solid-js";
import { useI18n } from "../i18n/index";

export function ConfirmDialog(props: {
  open: boolean;
  title: string;
  message: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
  danger?: boolean;
}) {
  const { t } = useI18n();

  function handleOverlayClick(e: MouseEvent) {
    if (e.target === e.currentTarget) {
      props.onCancel();
    }
  }

  return (
    <Show when={props.open}>
      <div class="modal-overlay" onClick={handleOverlayClick} role="dialog" aria-modal="true" aria-label={props.title}>
        <div class="modal-card">
          <div class="modal-title">{props.title}</div>
          <div class="modal-message">{props.message}</div>
          <div class="modal-actions">
            <button class="btn btn-secondary" onClick={props.onCancel}>
              {t("confirm.cancel")}
            </button>
            <button
              class={`btn ${props.danger ? "btn-danger" : "btn-primary"}`}
              onClick={props.onConfirm}
            >
              {props.confirmLabel}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}
