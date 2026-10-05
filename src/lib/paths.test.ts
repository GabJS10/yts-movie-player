import { describe, expect, it } from "vitest";
import { isWindowsPath, pathSeparator, samePath, splitPathTail, trimTrailingSeparators } from "./paths";

const WIN = "C:\\Users\\x\\AppData\\Local\\yts-player";

describe("paths", () => {
  it("tells Windows paths from POSIX ones", () => {
    expect(isWindowsPath(WIN)).toBe(true);
    expect(isWindowsPath("d:/Películas")).toBe(true);
    expect(isWindowsPath("E:")).toBe(true);
    expect(isWindowsPath("\\\\nas\\pelis\\2024")).toBe(true);
    expect(isWindowsPath("/home/usuario/.local/share/yts-player")).toBe(false);
    expect(isWindowsPath("/media/usb/back\\slash")).toBe(false);
    expect(pathSeparator(WIN)).toBe("\\");
    expect(pathSeparator("/media/usb")).toBe("/");
  });

  it("trims trailing separators but keeps roots", () => {
    expect(trimTrailingSeparators(`${WIN}\\library\\`)).toBe(`${WIN}\\library`);
    expect(trimTrailingSeparators("/media/usb//")).toBe("/media/usb");
    expect(trimTrailingSeparators("C:\\")).toBe("C:\\");
    expect(trimTrailingSeparators("/")).toBe("/");
    expect(trimTrailingSeparators("\\\\nas\\pelis\\")).toBe("\\\\nas\\pelis\\");
  });

  describe("splitPathTail", () => {
    it("keeps the last folder whole on Windows", () => {
      expect(splitPathTail(`${WIN}\\library`)).toEqual([`${WIN}\\`, "library"]);
      expect(splitPathTail(`${WIN}\\cache\\`)).toEqual([`${WIN}\\`, "cache"]);
      expect(splitPathTail("D:\\Películas")).toEqual(["D:\\", "Películas"]);
      expect(splitPathTail("D:/Películas/2024")).toEqual(["D:/Películas/", "2024"]);
    });

    it("handles roots, UNC shares and POSIX paths", () => {
      expect(splitPathTail("C:\\")).toEqual(["", "C:\\"]);
      expect(splitPathTail("/")).toEqual(["", "/"]);
      expect(splitPathTail("\\\\nas\\pelis\\2024")).toEqual(["\\\\nas\\pelis\\", "2024"]);
      expect(splitPathTail("/home/usuario/.local/share/yts-player/library")).toEqual([
        "/home/usuario/.local/share/yts-player/",
        "library",
      ]);
      expect(splitPathTail("/media/usb/Mis películas (2024)")).toEqual([
        "/media/usb/",
        "Mis películas (2024)",
      ]);
    });

    it("joins back into the trimmed path", () => {
      for (const p of [`${WIN}\\library`, "C:\\", "/", "/a/b/", "\\\\nas\\pelis\\x"]) {
        expect(splitPathTail(p).join("")).toBe(trimTrailingSeparators(p));
      }
    });
  });

  it("compares Windows paths ignoring case, slash style and trailing separators", () => {
    expect(samePath(`${WIN}\\library`, "c:\\users\\X\\appdata\\local\\YTS-PLAYER\\Library\\")).toBe(true);
    expect(samePath(`${WIN}\\library`, "C:/Users/x/AppData/Local/yts-player/library")).toBe(true);
    expect(samePath(`${WIN}\\library`, `${WIN}\\cache`)).toBe(false);
    expect(samePath("/media/usb/", "/media/usb")).toBe(true);
    // Linux is case-sensitive.
    expect(samePath("/media/USB", "/media/usb")).toBe(false);
  });
});
