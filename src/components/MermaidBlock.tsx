import { createSignal, onMount, Show } from "solid-js";
import { useI18n } from "../i18n/index";
import { CodeBlock } from "./CodeBlock";

let mermaidMod: typeof import("mermaid").default | null = null;
let renderCounter = 0;

function isDarkMode(): boolean {
  const attr = document.documentElement.getAttribute("data-theme");
  if (attr === "dark") return true;
  if (attr === "light") return false;
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function MermaidBlock(props: { code: string }) {
  const { t } = useI18n();
  const [html, setHtml] = createSignal<string | null>(null);
  const [error, setError] = createSignal(false);
  const [showSource, setShowSource] = createSignal(false);

  onMount(() => {
    renderDiagram();
  });

  async function renderDiagram() {
    try {
      if (!mermaidMod) {
        const mod = await import("mermaid");
        mermaidMod = mod.default;
      }
      // Re-initialize with current theme every render
      mermaidMod.initialize({
        startOnLoad: false,
        theme: isDarkMode() ? "dark" : "default",
        securityLevel: "strict",
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
      });
      const id = `mermaid-render-${++renderCounter}`;
      const { svg } = await mermaidMod.render(id, props.code);
      setHtml(svg);
      setError(false);
    } catch (e) {
      console.warn("Mermaid render failed:", e);
      setError(true);
    }
  }

  return (
    <Show
      when={!error()}
      fallback={<CodeBlock code={props.code} language="mermaid" />}
    >
      <div class="mermaid-block">
        <div class="mermaid-toolbar">
          <button
            class="mermaid-toggle"
            onClick={() => setShowSource((v) => !v)}
          >
            {showSource() ? t("common.viewDiagram") : t("common.viewSource")}
          </button>
        </div>
        {/* Security: innerHTML is used here to render Mermaid SVG output.
            Mermaid's "strict" securityLevel sanitizes the SVG (removes scripts,
            foreign objects, and event handlers), so this is considered safe. */}
        <Show
          when={showSource()}
          // eslint-disable-next-line solid/no-innerhtml -- Mermaid strict securityLevel sanitizes SVG
          fallback={<div class="mermaid-diagram" innerHTML={html() || ""} />}
        >
          <CodeBlock code={props.code} language="mermaid" />
        </Show>
      </div>
    </Show>
  );
}
