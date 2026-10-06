import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach } from "vitest";
import { setLanguagePreference } from "../i18n";
import { setMockTrailerMode, setMockUpdate } from "../mocks/backend";
import { resetConnectivity } from "../store/connectivity";
import { resetMove } from "../store/moveDownloads";
import { resetSearchMemory } from "../store/searchMemory";
import { resetUpdateNotice } from "../store/updateNotice";
import { clearQuota } from "../store/subtitlesQuota";
import { useToastStore } from "../store/toast";
import { installIntersectionObserver } from "./intersection";

installIntersectionObserver();

// Tests assert the Spanish copy; jsdom reports en-US, so pin the language instead of following it.
setLanguagePreference("es");

// findBy*/waitFor default to 1 s; lazy route chunks + the mock IPC can exceed it in a loaded parallel run.
configure({ asyncUtilTimeout: 4000 });

afterEach(() => {
  cleanup();
  clearMocks();
  useToastStore.setState({ toast: null });
  clearQuota();
  resetConnectivity();
  resetMove();
  resetSearchMemory();
  resetUpdateNotice();
  setMockUpdate(null);
  setMockTrailerMode("youtube");
  setLanguagePreference("es");
});

// jsdom has no media playback: play()/pause() flip `paused` and fire the matching events.
Object.defineProperty(HTMLMediaElement.prototype, "paused", {
  configurable: true,
  get(this: HTMLMediaElement & { _paused?: boolean }) {
    return this._paused ?? true;
  },
});
HTMLMediaElement.prototype.play = function (this: HTMLMediaElement & { _paused?: boolean }) {
  this._paused = false;
  this.dispatchEvent(new Event("playing"));
  return Promise.resolve();
};
HTMLMediaElement.prototype.pause = function (this: HTMLMediaElement & { _paused?: boolean }) {
  this._paused = true;
  this.dispatchEvent(new Event("pause"));
};
