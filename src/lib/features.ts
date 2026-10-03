import { isTauri } from "./runtime";

// Features whose backend commands land in a later phase (docs/ROADMAP.md). The browser mock already
// implements them; in the Tauri app they stay off so the UI never calls a command that doesn't exist yet.

/** Downloads (list_downloads, start_download…): phase 6. */
export const DOWNLOADS_READY = !isTauri();
