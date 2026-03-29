import { createSignal, Show } from "solid-js";

export function ThinkingBlock(props: { content: string }) {
  const [expanded, setExpanded] = createSignal(false);
  const preview = () => {
    const first = props.content.split("\n")[0];
    return first.length > 80 ? first.slice(0, 80) + "..." : first;
  };

  return (
    <div class={`msg-thinking${expanded() ? " expanded" : ""}`}>
      <div class="msg-thinking-header" onClick={() => setExpanded(!expanded())}>
        <span class="msg-thinking-icon">💭</span>
        <span class="msg-thinking-label">Thinking</span>
        <Show when={!expanded()}>
          <span class="msg-thinking-preview">{preview()}</span>
        </Show>
        <span class="msg-thinking-chevron">{expanded() ? "▾" : "▸"}</span>
      </div>
      <Show when={expanded()}>
        <pre class="msg-thinking-content">{props.content}</pre>
      </Show>
    </div>
  );
}
