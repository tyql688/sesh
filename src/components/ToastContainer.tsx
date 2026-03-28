import { For } from "solid-js";
import { toasts, type Toast } from "../stores/toast";

function toastIcon(type: Toast["type"]): string {
  switch (type) {
    case "success": return "✓";
    case "error": return "✕";
    case "info": return "ℹ";
  }
}

export function ToastContainer() {
  return (
    <div class="toast-container">
      <For each={toasts()}>
        {(t) => (
          <div class={`toast toast-${t.type}`}>
            <span class="toast-icon">{toastIcon(t.type)}</span>
            <span class="toast-message">{t.message}</span>
          </div>
        )}
      </For>
    </div>
  );
}
