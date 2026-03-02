// ---------------------------------------------------------------------------
// Safe display encoding — prevents XSS from user/market strings
// ---------------------------------------------------------------------------
//
// All strings rendered in the UI should go through these helpers to
// ensure safe display. React's JSX auto-escapes by default, but these
// provide explicit defense-in-depth for non-JSX contexts (logs, titles, etc).
// ---------------------------------------------------------------------------

/**
 * Sanitize a string for safe display in HTML context.
 * Escapes &, <, >, ", and ' characters.
 */
export function escapeHtml(str: string): string {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#x27;");
}

/**
 * Validate and sanitize a symbol string.
 * Symbols should only contain alphanumeric chars, /, -, _, and spaces.
 */
export function sanitizeSymbol(symbol: string): string {
  // Strip any character that isn't alphanumeric, slash, dash, underscore, or space
  return symbol.replace(/[^a-zA-Z0-9/_\-\s]/g, "");
}

/**
 * Validate and sanitize an order ID or account ID (UUID-like strings).
 * Only allows alphanumeric, dashes, and underscores.
 */
export function sanitizeId(id: string): string {
  return id.replace(/[^a-zA-Z0-9\-_]/g, "");
}

/**
 * Validate a string as a safe decimal for display.
 * Returns the string if valid, or "0" if not.
 */
export function safeDecimalDisplay(value: string): string {
  if (/^-?\d+(\.\d+)?$/.test(value)) {
    return value;
  }
  return "0";
}

/**
 * Truncate a string to maxLen, appending "…" if truncated.
 */
export function truncateDisplay(str: string, maxLen: number): string {
  if (str.length <= maxLen) return str;
  return str.slice(0, maxLen) + "…";
}
