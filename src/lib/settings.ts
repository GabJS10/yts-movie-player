// Client-side checks for Ajustes before calling update_settings (the backend validates again).

/** Same cap as the backend (src-tauri/src/settings.rs). */
export const MAX_API_BASE_URLS = 10;

/** A base URL the backend can use: absolute http(s). Returns an error message, or null if valid. */
export function validateBaseUrl(raw: string, existing: readonly string[]): string | null {
  const value = raw.trim();
  if (!value) return "Escribe la URL del servidor.";
  if (existing.length >= MAX_API_BASE_URLS)
    return `Caben ${MAX_API_BASE_URLS} servidores como máximo: quita alguno.`;
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return "No es una URL válida. Ejemplo: https://otro-espejo.ejemplo/api/v2/";
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return "La URL tiene que empezar por https://";
  if (existing.includes(value)) return "Ese servidor ya está en la lista.";
  return null;
}

/** Empty = null (no limit / automatic). Returns an error message for anything else that isn't valid. */
export function parseOptionalInt(raw: string, min: number, max: number): number | null | string {
  const text = raw.trim();
  if (!text) return null;
  if (!/^\d+$/.test(text)) return "Escribe un número entero o déjalo vacío.";
  const n = Number(text);
  if (n < min || n > max) return `Entre ${min} y ${max}.`;
  return n;
}
