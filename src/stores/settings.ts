import { createSignal } from "solid-js";
import type { Provider } from "../lib/types";
import { detectTerminal } from "../lib/tauri";

export type TerminalApp =
  | "terminal"
  | "iterm2"
  | "ghostty"
  | "kitty"
  | "warp"
  | "wezterm"
  | "alacritty" // macOS
  | "windows-terminal"
  | "powershell"
  | "cmd"; // Windows

const storedTerminal = localStorage.getItem("cc-session-terminal") as TerminalApp | null;

const [terminalApp, setTerminalAppSignal] = createSignal<TerminalApp>(storedTerminal || "terminal");

// Auto-detect terminal on first launch
if (!storedTerminal) {
  detectTerminal()
    .then((detected) => {
      const valid: TerminalApp[] = ["terminal", "iterm2", "ghostty", "kitty", "warp", "wezterm", "alacritty", "windows-terminal", "powershell", "cmd"];
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
  (() => {
    try {
      return JSON.parse(localStorage.getItem("cc-session-disabled-providers") || "[]") as Provider[];
    } catch {
      return [] as Provider[];
    }
  })(),
);

export function toggleProvider(id: Provider) {
  setDisabledProvidersSignal((prev) => {
    const next = prev.includes(id) ? prev.filter((p) => p !== id) : [...prev, id];
    localStorage.setItem("cc-session-disabled-providers", JSON.stringify(next));
    return next;
  });
}

export { disabledProviders };

// Time grouping toggle
const [timeGrouping, setTimeGroupingSignal] = createSignal<boolean>(
  localStorage.getItem("cc-session-time-grouping") !== "false",
);

export function setTimeGrouping(v: boolean) {
  setTimeGroupingSignal(v);
  localStorage.setItem("cc-session-time-grouping", String(v));
}

export { timeGrouping };

// Blocked folders: sessions from these project paths are hidden
const [blockedFolders, setBlockedFoldersSignal] = createSignal<string[]>(
  (() => {
    try {
      return JSON.parse(localStorage.getItem("cc-session-blocked-folders") || "[]") as string[];
    } catch {
      return [] as string[];
    }
  })(),
);

export function addBlockedFolder(path: string) {
  setBlockedFoldersSignal((prev) => {
    if (prev.includes(path)) return prev;
    const next = [...prev, path];
    localStorage.setItem("cc-session-blocked-folders", JSON.stringify(next));
    return next;
  });
}

export function removeBlockedFolder(path: string) {
  setBlockedFoldersSignal((prev) => {
    const next = prev.filter((p) => p !== path);
    localStorage.setItem("cc-session-blocked-folders", JSON.stringify(next));
    return next;
  });
}

export function isPathBlocked(path: string): boolean {
  return blockedFolders().some((blocked) => path === blocked || path.startsWith(blocked + "/"));
}

export { blockedFolders };
