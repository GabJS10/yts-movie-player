import { describe, expect, it } from "vitest";
import { windowCaption } from "../../lib/player";

const MB = 1024 * 1024;

describe("windowCaption", () => {
  it("describes the window the 200 cells cover", () => {
    expect(windowCaption({ startByte: 0, endByte: 64 * MB })).toBe(
      "Próximos 64 MB desde la posición de lectura",
    );
    expect(windowCaption({ startByte: 500 * MB, endByte: 564 * MB })).toBe(
      "Próximos 64 MB desde la posición de lectura",
    );
    expect(windowCaption({ startByte: 0, endByte: 40 * MB })).toBe("El archivo entero (40 MB)");
    expect(windowCaption(null)).toBeNull();
  });
});
