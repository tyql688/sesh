import { createSignal, createEffect, Show } from "solid-js";
import { readImageBase64 } from "../../lib/tauri";
import { useI18n } from "../../i18n/index";

export function isLocalPath(source: string): boolean {
  return (
    !source.startsWith("data:") &&
    !source.startsWith("http://") &&
    !source.startsWith("https://") &&
    !source.startsWith("asset:")
  );
}

/** Inline component that loads a local image via IPC and renders it. */
export function LocalImage(props: {
  path: string;
  onPreview: (src: string) => void;
}) {
  const { t } = useI18n();
  const [src, setSrc] = createSignal<string | null>(null);

  createEffect(() => {
    readImageBase64(props.path)
      .then(setSrc)
      .catch((e) => {
        console.warn("failed to load image:", props.path, e);
        setSrc(null);
      });
  });

  return (
    <Show when={src()}>
      <div class="msg-image-wrap">
        <img
          src={src()!}
          alt={t("common.image")}
          class="msg-image"
          loading="lazy"
          decoding="async"
          draggable={false}
          onClick={() => props.onPreview(src()!)}
        />
      </div>
    </Show>
  );
}

export function ImagePreview(props: { src: string; onClose: () => void }) {
  const { t } = useI18n();
  return (
    <div class="image-preview-overlay" onClick={props.onClose}>
      <img
        src={props.src}
        class="image-preview-img"
        onClick={(e) => e.stopPropagation()}
      />
      <button
        class="image-preview-close"
        aria-label={t("common.closePreview")}
        onClick={props.onClose}
      >
        <svg
          width="20"
          height="20"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          viewBox="0 0 24 24"
        >
          <line x1="18" y1="6" x2="6" y2="18" />
          <line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    </div>
  );
}
