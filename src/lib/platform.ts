export const isMac =
  typeof navigator !== "undefined" && /Mac/.test(navigator.platform);
export const isWindows =
  typeof navigator !== "undefined" && /Win/.test(navigator.platform);
export const isLinux =
  typeof navigator !== "undefined" &&
  /Linux/.test(navigator.platform) &&
  !/Android/.test(navigator.userAgent);
