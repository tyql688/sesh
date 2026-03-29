export function parseTimestamp(ts: string | null): number | null {
  if (!ts) return null;
  const n = Number(ts);
  if (!isNaN(n) && n > 0) {
    // If it looks like seconds (< 2e10), convert to ms
    return n < 2e10 ? n * 1000 : n;
  }
  const d = Date.parse(ts);
  return isNaN(d) ? null : d;
}

export function formatTimeOnly(ms: number): string {
  const d = new Date(ms);
  return d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
}

export function formatTimestamp(epoch: number, locale?: string): string {
  if (!epoch) return "\u2014";
  const now = Date.now();
  const ts = epoch * 1000;
  const diffMs = now - ts;
  const diffSec = Math.floor(diffMs / 1000);
  const isZh = locale === "zh";

  if (diffSec < 60) {
    return isZh ? "\u521a\u521a" : "just now";
  }
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) {
    return isZh ? `${diffMin} \u5206\u949f\u524d` : `${diffMin} minutes ago`;
  }
  const diffHour = Math.floor(diffMin / 60);
  if (diffHour < 24) {
    return isZh ? `${diffHour} \u5c0f\u65f6\u524d` : `${diffHour} hours ago`;
  }
  const diffDay = Math.floor(diffHour / 24);
  if (diffDay < 7) {
    return isZh ? `${diffDay} \u5929\u524d` : `${diffDay} days ago`;
  }
  const d = new Date(ts);
  return d.toLocaleString();
}

export function fmtK(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

export function formatFileSize(bytes: number): string {
  if (!bytes) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
