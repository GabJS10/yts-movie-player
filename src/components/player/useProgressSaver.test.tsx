import { QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MovieDetail } from "../../api/types";
import { createMockBackend } from "../../mocks/backend";
import { toSummary } from "../../lib/movie";
import { installBackend, testQueryClient } from "../../test/render";
import { SAVE_EVERY_MS, useProgressSaver } from "./useProgressSaver";

const movie = toSummary(createMockBackend().handle("get_movie", { movieId: 1632 }) as MovieDetail);

/** A stand-in <video>: only what the hook reads. */
function fakeVideo(currentTime: number, paused: boolean, duration = 10_000) {
  return { currentTime, paused, duration } as HTMLVideoElement;
}

function setup(video: HTMLVideoElement | null, active = true) {
  const ipc = installBackend();
  const qc = testQueryClient();
  const ref = { current: video };
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={qc}>{children}</QueryClientProvider>
  );
  const hook = renderHook(({ on }) => useProgressSaver(ref, movie, on), {
    wrapper,
    initialProps: { on: active },
  });
  const saves = () =>
    ipc.calls
      .filter((c) => c.cmd === "save_progress")
      .map((c) => {
        const a = c.args as { positionS: number; durationS: number; movie: { id: number } };
        return [a.movie.id, a.positionS, a.durationS];
      });
  return { ...hook, ref, saves, qc };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("useProgressSaver", () => {
  it("saves every 10 s while the element really plays, and not while paused", async () => {
    const v = fakeVideo(120, false);
    const { saves } = setup(v);
    await act(() => vi.advanceTimersByTimeAsync(SAVE_EVERY_MS - 1));
    expect(saves()).toEqual([]);
    await act(() => vi.advanceTimersByTimeAsync(1));
    expect(saves()).toEqual([[1632, 120, 10_000]]);

    v.currentTime = 130;
    await act(() => vi.advanceTimersByTimeAsync(SAVE_EVERY_MS));
    expect(saves()).toHaveLength(2);

    Object.assign(v, { paused: true, currentTime: 135 });
    await act(() => vi.advanceTimersByTimeAsync(SAVE_EVERY_MS * 3));
    expect(saves()).toHaveLength(2);
  });

  it("doesn't save the same position twice nor the first seconds", async () => {
    const v = fakeVideo(3, false);
    const { saves } = setup(v);
    await act(() => vi.advanceTimersByTimeAsync(SAVE_EVERY_MS));
    expect(saves()).toEqual([]); // < 5 s: just opened
    v.currentTime = 60;
    await act(() => vi.advanceTimersByTimeAsync(SAVE_EVERY_MS * 2)); // stuck at 60 (stall)
    expect(saves()).toEqual([[1632, 60, 10_000]]);
  });

  it("is idle until the video is shown (buffer screen)", async () => {
    const { saves, rerender } = setup(fakeVideo(200, false), false);
    await act(() => vi.advanceTimersByTimeAsync(SAVE_EVERY_MS * 2));
    expect(saves()).toEqual([]);
    rerender({ on: true });
    await act(() => vi.advanceTimersByTimeAsync(SAVE_EVERY_MS));
    expect(saves()).toHaveLength(1);
  });

  it("saves on pause, at the end (as the full duration) and when leaving", async () => {
    const v = fakeVideo(500, true, 6000);
    const { result, saves, unmount, ref } = setup(v);
    act(() => result.current.onPause());
    expect(saves()).toEqual([[1632, 500, 6000]]);

    v.currentTime = 5990;
    act(() => result.current.onEnded());
    expect(saves()[1]).toEqual([1632, 6000, 6000]);

    // Leaving: the element is gone by then; the last tracked position is used.
    v.currentTime = 700;
    act(() => result.current.track());
    ref.current = null;
    unmount();
    expect(saves()[2]).toEqual([1632, 700, 6000]);
  });

  it("falls back to the runtime when the duration is unknown", async () => {
    const v = fakeVideo(90, true, Number.NaN);
    const { result, saves } = setup(v);
    act(() => result.current.onPause());
    expect(saves()).toEqual([[1632, 90, movie.runtimeMin * 60]]);
  });

  it("puts the saved progress on the cached movie", async () => {
    const v = fakeVideo(321, true);
    const { result, qc } = setup(v);
    const detail = createMockBackend().handle("get_movie", { movieId: 1632 }) as MovieDetail;
    qc.setQueryData(["movie", 1632], detail);
    act(() => result.current.onPause());
    await act(() => vi.advanceTimersByTimeAsync(0));
    expect(qc.getQueryData<MovieDetail>(["movie", 1632])?.progress?.positionS).toBe(321);
  });
});
