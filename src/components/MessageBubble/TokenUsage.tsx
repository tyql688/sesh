import { createSignal, Show } from "solid-js";
import type { TokenUsage } from "../../lib/types";
import { useI18n } from "../../i18n/index";

export function CopyMessageButton(props: { content: string }) {
  const { t } = useI18n();
  const [copied, setCopied] = createSignal(false);

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(props.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // clipboard unavailable
    }
  }

  return (
    <button
      class="msg-copy-btn"
      onClick={handleCopy}
      title={t("common.copyMessage")}
      aria-label={t("common.copyMessage")}
    >
      {copied() ? (
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <polyline points="20 6 9 17 4 12" />
        </svg>
      ) : (
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
          <path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1" />
        </svg>
      )}
    </button>
  );
}

export function TokenUsageDisplay(props: { usage: TokenUsage }) {
  const fmt = (n: number) => n.toLocaleString();
  const cached = () => props.usage.cache_read_input_tokens;
  const created = () => props.usage.cache_creation_input_tokens;
  return (
    <div class="msg-token-usage">
      <span title="Input tokens">↑{fmt(props.usage.input_tokens)}</span>
      <span class="msg-token-sep">·</span>
      <span title="Output tokens">↓{fmt(props.usage.output_tokens)}</span>
      <Show when={cached() > 0}>
        <span class="msg-token-sep">·</span>
        <span class="msg-token-cached" title="Cache read tokens">cache_read {fmt(cached())}</span>
      </Show>
      <Show when={created() > 0}>
        <span class="msg-token-sep">·</span>
        <span class="msg-token-cache-write" title="Cache creation tokens">cache_write {fmt(created())}</span>
      </Show>
    </div>
  );
}
