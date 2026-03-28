import { createSignal, For, Show } from "solid-js";
import type { Message } from "../lib/types";
import { MessageBubble, formatMcpLabel } from "./MessageBubble";

export function MergedToolRow(props: { tools: string[]; messages: Message[]; highlightTerm?: string }) {
  const [expanded, setExpanded] = createSignal(false);

  const label = () => props.tools.length > 0 ? props.tools.map(formatMcpLabel).join(", ") : "tools";

  return (
    <div class="merged-tools">
      <div class="merged-tools-header" onClick={() => setExpanded(!expanded())}>
        <span class="msg-tool-icon">&#9881;</span>
        <span class="merged-tools-label">{label()}</span>
        <span class="merged-tools-chevron">{expanded() ? "\u25BE" : "\u25B8"}</span>
      </div>
      <Show when={expanded()}>
        <div class="merged-tools-body">
          <For each={props.messages}>
            {(msg) => <MessageBubble message={msg} highlightTerm={props.highlightTerm} />}
          </For>
        </div>
      </Show>
    </div>
  );
}
