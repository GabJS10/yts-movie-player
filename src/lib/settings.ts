import { getT } from "../i18n";

// Client-side checks for Ajustes before calling update_settings (the backend validates again).

/** Same cap as the backend (src-tauri/src/settings.rs). */
export const MAX_API_BASE_URLS = 10;

/** A base URL the backend can use: absolute http(s). Returns an error message, or null if valid. */
export function validateBaseUrl(raw: string, existing: readonly string[]): string | null {
  const value = raw.trim();
  const t = getT().validation;
  if (!value) return t.urlRequired;
  if (existing.length >= MAX_API_BASE_URLS) return t.tooManyUrls(MAX_API_BASE_URLS);
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return t.invalidUrl;
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return t.urlScheme;
  if (existing.includes(value)) return t.duplicateUrl;
  return null;
}

/** Empty = null (no limit / automatic). Returns an error message for anything else that isn't valid. */
export function parseOptionalInt(raw: string, min: number, max: number): number | null | string {
  const text = raw.trim();
  if (!text) return null;
  const t = getT().validation;
  if (!/^\d+$/.test(text)) return t.integerOrEmpty;
  const n = Number(text);
  if (n < min || n > max) return t.between(min, max);
  return n;
}
