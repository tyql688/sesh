import { createSignal } from "solid-js";
import type { Provider } from "../lib/types";
import { detectTerminal } from "../lib/tauri";

export type TerminalApp =
  | "terminal" | "iterm2" | "ghostty" | "kitty" | "warp" | "wezterm" | "alacritty"  // macOS
  | "windows-terminal" | "powershell" | "cmd";  // Windows

const storedTerminal = localStorage.getItem("cc-session-terminal") as TerminalApp | null;

const [terminalApp, setTerminalAppSignal] = createSignal<TerminalApp>(storedTerminal || "terminal");

// Auto-detect terminal on first launch
if (!storedTerminal) {
  detectTerminal()
    .then((detected) => {
      const valid: TerminalApp[] = ["terminal", "iterm2", "ghostty", "kitty", "warp", "wezterm", "alacritty"];
      if (valid.includes(detected as TerminalApp)) {
        setTerminalAppSignal(detected as TerminalApp);
        localStorage.setItem("cc-session-terminal", detected);
      }
    })
    .catch(() => {});
}

export function setTerminalApp(t: TerminalApp) {
  setTerminalAppSignal(t);
  localStorage.setItem("cc-session-terminal", t);
}

export { terminalApp };

// Provider toggle: store disabled providers in localStorage
const [disabledProviders, setDisabledProvidersSignal] = createSignal<Provider[]>(
  JSON.parse(localStorage.getItem("cc-session-disabled-providers") || "[]") as Provider[]
);

export function toggleProvider(id: Provider) {
  setDisabledProvidersSignal((prev) => {
    const next = prev.includes(id)
      ? prev.filter((p) => p !== id)
      : [...prev, id];
    localStorage.setItem("cc-session-disabled-providers", JSON.stringify(next));
    return next;
  });
}

export { disabledProviders };

// Time grouping toggle
const [timeGrouping, setTimeGroupingSignal] = createSignal<boolean>(
  localStorage.getItem("cc-session-time-grouping") === "true"
);

export function setTimeGrouping(v: boolean) {
  setTimeGroupingSignal(v);
  localStorage.setItem("cc-session-time-grouping", String(v));
}

export { timeGrouping };
