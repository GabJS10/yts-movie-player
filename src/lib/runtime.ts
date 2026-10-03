/** True inside the Tauri WebView; false in a plain browser (dev with mocks) or tests. */
export const isTauri = (): boolean => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
