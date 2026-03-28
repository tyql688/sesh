import { createSignal, createEffect, Show } from "solid-js";
import { useI18n } from "../i18n/index";

export function InputDialog(props: {
  open: boolean;
  title: string;
  label: string;
  defaultValue: string;
  confirmLabel: string;
  onConfirm: (value: string) => void;
  onCancel: () => void;
}) {
  const { t } = useI18n();
  const [value, setValue] = createSignal(props.defaultValue);
  let inputRef: HTMLInputElement | undefined;

  createEffect(() => {
    if (props.open) {
      setValue(props.defaultValue);
      // Focus input after render
      requestAnimationFrame(() => {
        inputRef?.focus();
        inputRef?.select();
      });
    }
  });

  function handleOverlayClick(e: MouseEvent) {
    if (e.target === e.currentTarget) {
      props.onCancel();
    }
  }

  function handleSubmit() {
    const trimmed = value().trim();
    if (trimmed && trimmed !== props.defaultValue) {
      props.onConfirm(trimmed);
    } else {
      props.onCancel();
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      handleSubmit();
    } else if (e.key === "Escape") {
      props.onCancel();
    }
  }

  return (
    <Show when={props.open}>
      <div class="modal-overlay" onClick={handleOverlayClick} role="dialog" aria-modal="true" aria-label={props.title}>
        <div class="modal-card">
          <div class="modal-title">{props.title}</div>
          <div class="modal-message">{props.label}</div>
          <input
            ref={inputRef}
            class="modal-input"
            type="text"
            value={value()}
            onInput={(e) => setValue(e.currentTarget.value)}
            onKeyDown={handleKeyDown}
          />
          <div class="modal-actions">
            <button class="btn btn-secondary" onClick={props.onCancel}>
              {t("confirm.cancel")}
            </button>
            <button class="btn btn-primary" onClick={handleSubmit}>
              {props.confirmLabel}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}
