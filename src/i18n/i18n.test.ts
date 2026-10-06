import { describe, expect, it } from "vitest";
import { en } from "./en";
import { es } from "./es";
import { langName } from "../lib/subtitles";
import { detectLanguage, getT, setLanguagePreference, useLanguageStore } from ".";

/** Every leaf of a dictionary, as dotted paths. */
const leaves = (obj: object, prefix = ""): string[] =>
  Object.entries(obj).flatMap(([k, v]) =>
    v && typeof v === "object" ? leaves(v as object, `${prefix}${k}.`) : [`${prefix}${k}`],
  );

describe("i18n", () => {
  it("follows the first Spanish or English system language, English otherwise", () => {
    expect(detectLanguage(["es-ES", "en-US"])).toBe("es");
    expect(detectLanguage(["es_MX"])).toBe("es");
    expect(detectLanguage(["fr-FR", "en-GB", "es"])).toBe("en");
    expect(detectLanguage(["fr-FR", "de"])).toBe("en");
    expect(detectLanguage([])).toBe("en");
  });

  it("'system' resolves to the system language; an explicit choice wins", () => {
    // jsdom reports en-US.
    setLanguagePreference("system");
    expect(useLanguageStore.getState()).toMatchObject({ preference: "system", language: "en" });
    expect(getT()).toBe(en);
    setLanguagePreference("es");
    expect(useLanguageStore.getState()).toMatchObject({ preference: "es", language: "es" });
    expect(getT()).toBe(es);
  });

  it("English has the same keys as Spanish, and no empty strings", () => {
    expect(leaves(en).sort()).toEqual(leaves(es).sort());
    for (const dict of [es, en])
      for (const path of leaves(dict)) {
        const value = path.split(".").reduce<unknown>((o, k) => (o as Record<string, unknown>)[k], dict);
        if (typeof value === "string") expect(value.trim(), path).not.toBe("");
      }
  });

  it("names subtitle languages in the UI language for running text", () => {
    setLanguagePreference("en");
    expect(langName("es")).toBe("Spanish");
    setLanguagePreference("es");
    expect(langName("pt")).toBe("portugués");
  });
});
