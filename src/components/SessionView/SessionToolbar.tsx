import { Show } from "solid-js";
import type { Accessor } from "solid-js";
import type { SessionMeta, Message } from "../../lib/types";
import { useI18n } from "../../i18n/index";
import { getProviderLabel } from "../../stores/providerSnapshots";
import {
  formatTimestamp,
  fmtK,
  formatFileSize,
  shortenHomePath,
} from "../../lib/formatters";
import type { ProcessedEntry } from "./hooks";

export function SessionToolbar(props: {
  meta: Accessor<SessionMeta>;
  messages: Accessor<Message[]>;
  processedEntries: Accessor<ProcessedEntry[]>;
  watching: Accessor<boolean>;
  starred: Accessor<boolean>;
  onToggleWatch: () => void;
  onToggleFavorite: () => void;
  onResume: () => void;
  onExport: () => void;
  onCopy: () => void;
  onDelete: () => void;
}) {
  const { t, locale } = useI18n();

  const providerLabel = () => {
    const meta = props.meta();
    return getProviderLabel(meta.provider, meta.variant_name);
  };

  // Total token usage across all messages
  const totalTokens = () => {
    let input = 0,
      output = 0,
      cacheRead = 0,
      cacheWrite = 0;
    for (const e of props.processedEntries()) {
      const msgs =
        e.type === "message"
          ? [e.msg]
          : e.type === "merged-tools"
            ? e.messages
            : [];
      for (const m of msgs) {
        if (m.token_usage) {
          input += m.token_usage.input_tokens;
          output += m.token_usage.output_tokens;
          cacheRead += m.token_usage.cache_read_input_tokens;
          cacheWrite += m.token_usage.cache_creation_input_tokens;
        }
      }
    }
    return input + output > 0 ? { input, output, cacheRead, cacheWrite } : null;
  };

  return (
    <>
      {/* Header */}
      <div class="session-header">
        <div class="session-breadcrumb">
          <div class="breadcrumb-nav">
            <span
              class="breadcrumb-provider"
              style={{ color: `var(--${props.meta().provider})` }}
            >
              {providerLabel()}
            </span>
            <span class="breadcrumb-sep">&rsaquo;</span>
            <span class="breadcrumb-project">
              {props.meta().project_name || t("explorer.noProject")}
            </span>
          </div>
          <div class="breadcrumb-title">{props.meta().title}</div>
        </div>
        <div class="session-actions">
          <button
            class={`session-action-btn session-action-btn-icon${props.watching() ? " watching" : ""}`}
            onClick={props.onToggleWatch}
            title={
              props.watching()
                ? t("session.watchStop")
                : t("session.watchStart")
            }
          >
            {props.watching() ? "\u25C9" : "\u25CE"}
          </button>
          <button
            class={`session-action-btn session-action-btn-icon${props.starred() ? " starred" : ""}`}
            onClick={props.onToggleFavorite}
            title={
              props.starred()
                ? t("session.favoriteRemove")
                : t("session.favoriteAdd")
            }
          >
            {props.starred() ? "\u2605" : "\u2606"}
          </button>
          <button
            class="session-action-btn primary"
            onClick={props.onResume}
            title={t("session.resume")}
          >
            {t("session.resume")}
          </button>
          <button
            class="session-action-btn"
            onClick={props.onExport}
            title={t("session.export")}
          >
            {t("session.export")}
          </button>
          <button
            class="session-action-btn"
            onClick={props.onCopy}
            title={t("session.copy")}
          >
            {t("session.copy")}
          </button>
          <button
            class="session-action-btn session-action-btn-danger"
            onClick={props.onDelete}
            title={t("session.delete")}
          >
            {t("session.delete")}
          </button>
        </div>
      </div>

      {/* Info bar */}
      <div class="session-info">
        <span>
          {t("session.created")}:{" "}
          {formatTimestamp(props.meta().created_at, locale())}
        </span>
        <span class="info-sep">&middot;</span>
        <span>
          {props.meta().message_count || props.messages().length}{" "}
          {t("session.messages")}
        </span>
        <span class="info-sep">&middot;</span>
        <span>{formatFileSize(props.meta().file_size_bytes)}</span>
        <Show when={totalTokens()}>
          <span class="info-sep">&middot;</span>
          <span
            class="session-info-tokens"
            title={`${t("common.inputTokens")}: ${totalTokens()!.input.toLocaleString()}, ${t("common.outputTokens")}: ${totalTokens()!.output.toLocaleString()}${totalTokens()!.cacheWrite > 0 ? `, ${t("common.cacheWriteTokens")}: ${totalTokens()!.cacheWrite.toLocaleString()}` : ""}${totalTokens()!.cacheRead > 0 ? `, ${t("common.cacheReadTokens")}: ${totalTokens()!.cacheRead.toLocaleString()}` : ""}`}
          >
            {"\u2191"}
            {fmtK(totalTokens()!.input)} {"\u2193"}
            {fmtK(totalTokens()!.output)} {t("common.tokens")}
            <Show
              when={totalTokens()!.cacheWrite + totalTokens()!.cacheRead > 0}
            >
              {" · "}
              <span class="cache-read-label">
                {t("common.cacheRead")} {fmtK(totalTokens()!.cacheRead)}
              </span>
              {" · "}
              {t("common.cacheWrite")} {fmtK(totalTokens()!.cacheWrite)}
            </Show>
          </span>
        </Show>
        <Show when={props.meta().is_sidechain}>
          <span class="info-sep">&middot;</span>
          <span class="session-info-sidechain">
            {"\u2937"} {t("session.subagent")}
          </span>
        </Show>
        <Show when={props.meta().model}>
          <span class="info-sep">&middot;</span>
          <span class="session-info-model" title={props.meta().model}>
            {props.meta().model}
          </span>
        </Show>
        <Show when={props.meta().cc_version}>
          <span class="info-sep">&middot;</span>
          <span class="session-info-version">v{props.meta().cc_version}</span>
        </Show>
        <Show when={props.meta().git_branch}>
          <span class="info-sep">&middot;</span>
          <span class="session-info-branch" title={props.meta().git_branch}>
            {"\u2387"} {props.meta().git_branch}
          </span>
        </Show>
        <Show when={props.meta().project_path}>
          <span class="info-sep">&middot;</span>
          <span class="session-info-path" title={props.meta().project_path}>
            {shortenHomePath(props.meta().project_path)}
          </span>
        </Show>
      </div>
    </>
  );
}
