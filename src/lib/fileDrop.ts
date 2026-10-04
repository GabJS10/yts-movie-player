import { isTauri } from "./runtime";

export type FileDropEvent =
  { type: "enter"; paths: string[] } | { type: "drop"; paths: string[] } | { type: "leave" };

/**
 * Files dragged onto the window, with their real paths (Tauri's onDragDropEvent; a plain browser
 * gives no paths, so outside Tauri this is a no-op). Resolves to the unsubscribe function.
 */
export async function onFileDrop(handler: (e: FileDropEvent) => void): Promise<() => void> {
  if (!isTauri()) return () => undefined;
  try {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    return await getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === "enter" || payload.type === "drop")
        handler({ type: payload.type, paths: payload.paths });
      else if (payload.type === "leave") handler({ type: "leave" });
    });
  } catch (err) {
    console.warn("drag & drop unavailable", err);
    return () => undefined;
  }
}
