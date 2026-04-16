import { createEffect, onMount, onCleanup } from "solid-js";
import type { ProcessedEntry } from "./hooks";

const ROLE_COLORS: Record<string, string> = {
  user: "#007aff",
  assistant: "#9ca3af",
  tool: "#10b981",
  system: "#f59e0b",
};

const MERGED_TOOL_WEIGHT = 20;
const MIN_BLOCK_HEIGHT = 2;
const MIN_ENTRIES_TO_SHOW = 10;

interface MinimapProps {
  entries: ProcessedEntry[];
  messagesRef: HTMLDivElement | undefined;
}

export function TimelineMinimap(props: MinimapProps) {
  let canvasRef: HTMLCanvasElement | undefined;
  let containerRef: HTMLDivElement | undefined;

  interface Block {
    y: number;
    h: number;
    color: string;
    entryIndex: number;
  }

  function computeBlocks(
    entries: ProcessedEntry[],
    canvasHeight: number,
  ): Block[] {
    const items: { color: string; weight: number; entryIndex: number }[] = [];
    for (let i = 0; i < entries.length; i++) {
      const e = entries[i];
      if (e.type === "time-sep") continue;
      if (e.type === "merged-tools") {
        items.push({
          color: ROLE_COLORS.tool,
          weight: MERGED_TOOL_WEIGHT,
          entryIndex: i,
        });
      } else {
        const weight = Math.max(1, e.msg.content?.length ?? 0);
        const color = ROLE_COLORS[e.msg.role] ?? ROLE_COLORS.assistant;
        items.push({ color, weight, entryIndex: i });
      }
    }
    if (items.length === 0) return [];

    const totalWeight = items.reduce((sum, it) => sum + it.weight, 0);
    const blocks: Block[] = [];
    let y = 0;
    for (const item of items) {
      const rawH = (item.weight / totalWeight) * canvasHeight;
      const h = Math.max(MIN_BLOCK_HEIGHT, rawH);
      blocks.push({ y, h, color: item.color, entryIndex: item.entryIndex });
      y += h;
    }
    if (y > canvasHeight && blocks.length > 0) {
      const scale = canvasHeight / y;
      let accY = 0;
      for (const b of blocks) {
        b.y = accY;
        b.h = Math.max(1, b.h * scale);
        accY += b.h;
      }
    }
    return blocks;
  }

  function drawBlocks(
    ctx: CanvasRenderingContext2D,
    blocks: Block[],
    width: number,
  ) {
    ctx.clearRect(0, 0, width, ctx.canvas.height);
    for (const b of blocks) {
      ctx.fillStyle = b.color;
      ctx.fillRect(0, b.y, width, b.h);
    }
  }

  function drawViewport(
    ctx: CanvasRenderingContext2D,
    width: number,
    canvasHeight: number,
  ) {
    const el = props.messagesRef;
    if (!el) return;

    const { scrollTop, scrollHeight, clientHeight } = el;
    if (scrollHeight <= clientHeight) return;

    const viewFraction = clientHeight / scrollHeight;
    const bottomFraction = -scrollTop / (scrollHeight - clientHeight);

    const indicatorH = Math.max(8, viewFraction * canvasHeight);
    const indicatorBottom =
      canvasHeight - bottomFraction * (canvasHeight - indicatorH);
    const indicatorY = indicatorBottom - indicatorH;

    ctx.fillStyle = "rgba(255, 255, 255, 0.15)";
    ctx.fillRect(0, indicatorY, width, indicatorH);
    ctx.strokeStyle = "rgba(255, 255, 255, 0.3)";
    ctx.lineWidth = 1;
    ctx.strokeRect(0.5, indicatorY + 0.5, width - 1, indicatorH - 1);
  }

  let currentBlocks: Block[] = [];

  function repaint() {
    const canvas = canvasRef;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);

    currentBlocks = computeBlocks(props.entries, rect.height);
    drawBlocks(ctx, currentBlocks, rect.width);
    drawViewport(ctx, rect.width, rect.height);
  }

  function repaintViewportOnly() {
    const canvas = canvasRef;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    drawBlocks(ctx, currentBlocks, rect.width);
    drawViewport(ctx, rect.width, rect.height);
  }

  function handleScroll() {
    repaintViewportOnly();
  }

  function handleCanvasClick(e: MouseEvent) {
    const canvas = canvasRef;
    const el = props.messagesRef;
    if (!canvas || !el) return;

    const rect = canvas.getBoundingClientRect();
    const clickY = e.clientY - rect.top;
    const canvasH = rect.height;

    const fraction = clickY / canvasH;
    const maxScroll = el.scrollHeight - el.clientHeight;
    el.scrollTop = -(1 - fraction) * maxScroll;
  }

  function handleCanvasMouseDown(e: MouseEvent) {
    e.preventDefault();
    const canvas = canvasRef;
    const el = props.messagesRef;
    if (!canvas || !el) return;

    const onMove = (me: MouseEvent) => {
      const rect = canvas.getBoundingClientRect();
      const fraction = Math.max(
        0,
        Math.min(1, (me.clientY - rect.top) / rect.height),
      );
      const maxScroll = el.scrollHeight - el.clientHeight;
      el.scrollTop = -(1 - fraction) * maxScroll;
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  onMount(() => {
    const el = props.messagesRef;
    if (el) {
      el.addEventListener("scroll", handleScroll);
    }

    const ro = new ResizeObserver(() => repaint());
    if (containerRef) ro.observe(containerRef);

    onCleanup(() => {
      if (el) el.removeEventListener("scroll", handleScroll);
      ro.disconnect();
    });
  });

  createEffect(() => {
    const _entries = props.entries;
    repaint();
  });

  return (
    <div
      class="timeline-minimap"
      ref={containerRef}
      style={{
        display:
          props.entries.length < MIN_ENTRIES_TO_SHOW ? "none" : undefined,
      }}
    >
      <canvas
        ref={canvasRef}
        class="timeline-minimap-canvas"
        onMouseDown={handleCanvasMouseDown}
        onClick={handleCanvasClick}
      />
    </div>
  );
}
