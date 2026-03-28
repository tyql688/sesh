import { createSignal } from "solid-js";

export type Theme = "light" | "dark" | "system";

function getInitialTheme(): Theme {
  const stored = localStorage.getItem("cc-session-theme");
  if (stored === "light" || stored === "dark") return stored;
  return "system";
}

export function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === "system") {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", theme);
  }
  localStorage.setItem("cc-session-theme", theme);
}

const [theme, setThemeSignal] = createSignal<Theme>(getInitialTheme());

export function setTheme(t: Theme) {
  setThemeSignal(t);
  applyTheme(t);
}

export { theme };
