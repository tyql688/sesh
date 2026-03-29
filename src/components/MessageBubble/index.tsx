import { createSignal, createMemo, For, Show, JSX } from "solid-js";
import type { Message, Provider } from "../../lib/types";
import { ProviderIcon, UserIcon } from "../../lib/icons";
import { CodeBlock } from "../CodeBlock";
import { MermaidBlock } from "../MermaidBlock";
import { parseContent } from "./MarkdownRenderer";
import { renderMarkdownText } from "./MarkdownRenderer";
import { LocalImage, ImagePreview, isLocalPath } from "./ImagePreview";
import { ThinkingBlock } from "./ThinkingBlock";
import { CopyMessageButton, TokenUsageDisplay } from "./TokenUsage";
import { ToolMessage } from "./ToolMessage";

// Re-export for backward compatibility
export { ProviderIcon } from "../../lib/icons";
export { formatMcpLabel } from "./ToolMessage";

export function MessageBubble(props: { message: Message; provider?: Provider; highlightTerm?: string }) {
  const segments = createMemo(() => parseContent(props.message.content));
  const [previewSrc, setPreviewSrc] = createSignal<string | null>(null);

  const isEmpty = (): boolean => {
    const msg = props.message;
    if (msg.role === "tool") {
      // Hide tool_result entries (toulu_ IDs from Anthropic API)
      if (msg.tool_name?.startsWith("toolu_")) return true;
      return !msg.content && !msg.tool_input && !msg.tool_name;
    }
    return !msg.content || msg.content.trim().length === 0;
  };

  const isSystemContent = (): boolean => {
    const msg = props.message;
    if (msg.role === "tool") return false;
    if (!msg.content || msg.content.trim().length === 0) return false;
    const c = msg.content.trimStart();
    // Skip known system/template content markers
    const systemMarkers = [
      "</observation>", "</command-message>", "<INSTRUCTIONS>",
      "<environment_context>", "<permissions instructions>",
      "</facts>", "</narrative>", "</concepts>",
      "<system-reminder>",
    ];
    return systemMarkers.some((marker) => c.includes(marker));
  };

  if (isEmpty() || isSystemContent()) return null;

  return (
    <>
    <Show
      when={props.message.role !== "tool"}
      fallback={<ToolMessage message={props.message} />}
    >
      <Show
        when={props.message.role !== "system"}
        fallback={
          props.message.content.startsWith("[thinking]\n")
            ? <ThinkingBlock content={props.message.content.slice("[thinking]\n".length)} />
            : <div class="msg-system">{props.message.content}</div>
        }
      >
        <div class={`msg-row msg-row-${props.message.role}`}>
          <div class={`msg-avatar msg-avatar-${props.message.role}${props.message.role === "assistant" ? ` ${props.provider ?? "claude"}` : ""}`}>
            <Show
              when={props.message.role === "user"}
              fallback={<ProviderIcon provider={props.provider ?? "claude"} />}
            >
              <UserIcon />
            </Show>
          </div>
          <div class={`msg-bubble msg-bubble-${props.message.role}`}>
            <For each={segments()}>
              {(seg) => {
                if (seg.type === "code") {
                  if (seg.language?.toLowerCase() === "mermaid") {
                    return <MermaidBlock code={seg.content} />;
                  }
                  return <CodeBlock code={seg.content} language={seg.language} />;
                }
                if (seg.type === "image") {
                  if (isLocalPath(seg.content)) {
                    return (
                      <LocalImage path={seg.content} onPreview={(s) => setPreviewSrc(s)} />
                    );
                  }
                  return (
                    <div class="msg-image-wrap">
                      <img
                        src={seg.content}
                        alt="Image"
                        class="msg-image"
                        loading="lazy"
                        decoding="async"
                        draggable={false}
                        onClick={() => setPreviewSrc(seg.content)}
                      />
                    </div>
                  );
                }
                // createMemo makes this reactive: re-renders when highlightTerm signal changes.
                // <For> callbacks only run once per item, so without memo the highlight would be static.
                return createMemo(() => renderMarkdownText(seg.content, props.highlightTerm)) as unknown as JSX.Element;
              }}
            </For>
            <CopyMessageButton content={props.message.content} />
          </div>
        </div>
        <Show when={props.message.role === "assistant" && props.message.token_usage}>
          <div class="msg-token-row">
            <TokenUsageDisplay usage={props.message.token_usage!} />
          </div>
        </Show>
      </Show>
    </Show>
    <Show when={previewSrc()}>
      <ImagePreview src={previewSrc()!} onClose={() => setPreviewSrc(null)} />
    </Show>
    </>
  );
}
