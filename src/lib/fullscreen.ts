import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "./runtime";

// Inside Tauri the window itself goes fullscreen (needs core:window:allow-set-fullscreen);
// if that's not permitted, or in a browser, the DOM Fullscreen API is used.

export async function isFullscreen(): Promise<boolean> {
  if (document.fullscreenElement) return true;
  if (isTauri()) {
    try {
      return await getCurrentWindow().isFullscreen();
    } catch {
      return false;
    }
  }
  return false;
}

export async function setFullscreen(on: boolean, el: HTMLElement): Promise<void> {
  if (!on && document.fullscreenElement) {
    await document.exitFullscreen();
    return;
  }
  if (isTauri()) {
    try {
      await getCurrentWindow().setFullscreen(on);
      return;
    } catch {
      // fall through to the DOM API
    }
  }
  if (on) await el.requestFullscreen?.();
}
