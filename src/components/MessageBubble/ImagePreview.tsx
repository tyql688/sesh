import { createSignal, createEffect, onCleanup, Show } from "solid-js";
import { readImageBase64 } from "../../lib/tauri";
import { cachedLoad } from "../../lib/image-cache";
import { shortenHomePath } from "../../lib/formatters";
import { useI18n } from "../../i18n/index";

export function isLocalPath(source: string): boolean {
  return (
    !source.startsWith("data:") &&
    !source.startsWith("http://") &&
    !source.startsWith("https://") &&
    !source.startsWith("asset:")
  );
}

export function LocalImage(props: {
  path: string;
  onPreview: (src: string, source: string) => void;
}) {
  const [src, setSrc] = createSignal<string | null>(null);
  const [failed, setFailed] = createSignal(false);

  createEffect(() => {
    let active = true;
    setSrc(null);
    setFailed(false);

    cachedLoad(props.path, () => readImageBase64(props.path))
      .then((loaded) => {
        if (!active) return;
        setSrc(loaded);
      })
      .catch((e) => {
        if (!active) return;
        console.warn("failed to load image:", props.path, e);
        setFailed(true);
      });

    onCleanup(() => {
      active = false;
    });
  });

  return (
    <Show
      when={src()}
      fallback={
        failed() ? (
          <div class="msg-image-wrap">
            <ImageFallback source={props.path} />
          </div>
        ) : (
          <div class="msg-image-wrap">
            <ImageLoading source={props.path} />
          </div>
        )
      }
    >
      <InlineImage
        src={src()!}
        source={props.path}
        onPreview={props.onPreview}
      />
    </Show>
  );
}

export function RemoteImage(props: {
  src: string;
  onPreview: (src: string, source: string) => void;
}) {
  const [loadedSrc, setLoadedSrc] = createSignal<string | null>(null);
  const [failed, setFailed] = createSignal(false);

  createEffect(() => {
    let active = true;
    setLoadedSrc(null);
    setFailed(false);

    cachedLoad(props.src, () => {
      return new Promise<string>((resolve, reject) => {
        const image = new Image();
        image.onload = () => resolve(props.src);
        image.onerror = () => reject(new Error("remote image load failed"));
        image.src = props.src;
      });
    })
      .then((src) => {
        if (!active) return;
        setLoadedSrc(src);
      })
      .catch(() => {
        if (!active) return;
        setFailed(true);
      });

    onCleanup(() => {
      active = false;
    });
  });

  return (
    <Show
      when={loadedSrc()}
      fallback={
        failed() ? (
          <div class="msg-image-wrap">
            <ImageFallback source={props.src} />
          </div>
        ) : (
          <div class="msg-image-wrap">
            <ImageLoading source={props.src} />
          </div>
        )
      }
    >
      <InlineImage
        src={loadedSrc()!}
        source={props.src}
        onPreview={props.onPreview}
      />
    </Show>
  );
}

function InlineImage(props: {
  src: string;
  source: string;
  onPreview: (src: string, source: string) => void;
}) {
  return (
    <div class="msg-image-wrap">
      <button
        type="button"
        class="msg-image-button"
        onClick={() => props.onPreview(props.src, props.source)}
        title={describeImageSource(props.source)}
      >
        <img
          src={props.src}
          alt={describeImageSource(props.source)}
          class="msg-image is-ready"
          loading="lazy"
          decoding="async"
          draggable={false}
        />
      </button>
    </div>
  );
}

function ImageLoading(props: { source: string }) {
  const { t } = useI18n();
  return (
    <div
      class="msg-image-state msg-image-loading"
      title={describeImageSource(props.source)}
    >
      <span class="msg-image-state-label">{t("common.loading")}</span>
      <span class="msg-image-state-source">
        {labelImageSource(props.source, t)}
      </span>
    </div>
  );
}

function ImageFallback(props: { source: string }) {
  const { t } = useI18n();
  return (
    <div
      class="msg-image-state msg-image-fallback"
      title={describeImageSource(props.source)}
    >
      <span class="msg-image-state-label">{t("common.imageLoadFailed")}</span>
      <span class="msg-image-state-source">
        {labelImageSource(props.source, t)}
      </span>
    </div>
  );
}

export function ImagePreview(props: {
  src: string;
  source?: string;
  onClose: () => void;
}) {
  const { t } = useI18n();

  createEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        props.onClose();
      }
    };

    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    window.addEventListener("keydown", onKeyDown);

    onCleanup(() => {
      document.body.style.overflow = previousOverflow;
      window.removeEventListener("keydown", onKeyDown);
    });
  });

  return (
    <div class="image-preview-overlay" onClick={props.onClose}>
      <img
        src={props.src}
        alt={t("common.image")}
        class="image-preview-img"
        onClick={(e) => e.stopPropagation()}
      />
      <Show when={props.source}>
        <div
          class="image-preview-meta"
          title={props.source ? describeImageSource(props.source) : undefined}
          onClick={(e) => e.stopPropagation()}
        >
          {labelImageSource(props.source!, t)}
        </div>
      </Show>
      <button
        type="button"
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

function labelImageSource(source: string, t: (key: string) => string): string {
  if (source.startsWith("data:")) {
    return t("common.embeddedImage");
  }

  if (source.startsWith("http://") || source.startsWith("https://")) {
    try {
      const url = new URL(source);
      const pathSegments = url.pathname.split("/").filter(Boolean);
      const tail = pathSegments.slice(-2).join("/");
      return tail ? `${url.hostname}/${tail}` : url.hostname;
    } catch {
      return source;
    }
  }

  const normalized = shortenHomePath(source).replace(/\\/g, "/");
  const pathSegments = normalized.split("/").filter(Boolean);
  if (normalized.startsWith("~/")) {
    return `~/${pathSegments.slice(-2).join("/")}`;
  }
  return pathSegments.slice(-2).join("/") || source;
}

function describeImageSource(source: string): string {
  if (source.startsWith("data:")) {
    return "embedded image";
  }
  return shortenHomePath(source).replaceAll("\\", "/");
}
