import { describe, it, expect } from "vitest";
import { parseTimestamp, fmtK, formatFileSize, formatTimestamp } from "./formatters";

describe("parseTimestamp", () => {
  it("parses epoch seconds and converts to ms", () => {
    expect(parseTimestamp("1711800000")).toBe(1711800000000);
  });
  it("passes through epoch ms", () => {
    expect(parseTimestamp("1711800000000")).toBe(1711800000000);
  });
  it("parses ISO 8601 string", () => {
    const result = parseTimestamp("2026-03-30T12:00:00Z");
    expect(result).toBeGreaterThan(0);
  });
  it("returns null for null input", () => {
    expect(parseTimestamp(null)).toBeNull();
  });
  it("returns null for invalid string", () => {
    expect(parseTimestamp("not-a-date")).toBeNull();
  });
});

describe("fmtK", () => {
  it("formats millions", () => {
    expect(fmtK(1_500_000)).toBe("1.5M");
  });
  it("formats thousands", () => {
    expect(fmtK(2_500)).toBe("2.5k");
  });
  it("returns raw number for small values", () => {
    expect(fmtK(42)).toBe("42");
  });
});

describe("formatFileSize", () => {
  it("formats bytes", () => {
    expect(formatFileSize(500)).toBe("500 B");
  });
  it("formats kilobytes", () => {
    expect(formatFileSize(2048)).toBe("2.0 KB");
  });
  it("formats megabytes", () => {
    expect(formatFileSize(1_500_000)).toBe("1.4 MB");
  });
  it("returns dash for zero", () => {
    expect(formatFileSize(0)).toBe("\u2014");
  });
});

describe("formatTimestamp", () => {
  it("returns dash for zero epoch", () => {
    expect(formatTimestamp(0)).toBe("\u2014");
  });
  it("returns 'just now' for recent epoch", () => {
    const nowEpoch = Math.floor(Date.now() / 1000);
    expect(formatTimestamp(nowEpoch)).toBe("just now");
  });
  it("returns Chinese for zh locale", () => {
    const nowEpoch = Math.floor(Date.now() / 1000);
    expect(formatTimestamp(nowEpoch, "zh")).toBe("\u521a\u521a");
  });
});
