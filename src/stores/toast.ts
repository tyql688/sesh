import { createSignal } from "solid-js";

export type ToastType = "success" | "error" | "info";

export interface Toast {
  id: number;
  type: ToastType;
  message: string;
}

let nextId = 0;
const toastTimers = new Map<number, ReturnType<typeof setTimeout>>();

const [toasts, setToasts] = createSignal<Toast[]>([]);

function removeToast(id: number) {
  setToasts((prev) => prev.filter((t) => t.id !== id));
  const timer = toastTimers.get(id);
  if (timer) {
    clearTimeout(timer);
    toastTimers.delete(id);
  }
}

function addToast(type: ToastType, message: string, duration = 3000) {
  const id = ++nextId;
  setToasts((prev) => [...prev, { id, type, message }]);
  const timer = setTimeout(() => removeToast(id), duration);
  toastTimers.set(id, timer);
}

function toast(message: string) {
  addToast("success", message);
}

function toastInfo(message: string) {
  addToast("info", message);
}

function toastError(message: string) {
  addToast("error", message, 5000);
}

export { toasts, toast, toastInfo, toastError };
