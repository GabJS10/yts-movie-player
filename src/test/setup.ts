import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach } from "vitest";
import { installIntersectionObserver } from "./intersection";

installIntersectionObserver();

afterEach(() => {
  cleanup();
  clearMocks();
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
