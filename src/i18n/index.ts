import { create } from "zustand";
import { en } from "./en";
import { es, type Messages } from "./es";

// UI language. The preference lives in localStorage (read synchronously at start, so the first
// paint is already in the right language); "system" follows the OS/WebView languages.
export type Language = "es" | "en";
export type LanguagePreference = Language | "system";
export type { Messages };

const MESSAGES: Record<Language, Messages> = { es, en };
const LOCALES: Record<Language, string> = { es: "es-ES", en: "en-US" };
const STORAGE_KEY = "yts-player:language";

/** First Spanish or English entry in the system languages; anything else falls back to English. */
export function detectLanguage(languages: readonly string[] = systemLanguages()): Language {
  for (const tag of languages) {
    const base = tag.toLowerCase().split(/[-_]/)[0];
    if (base === "es" || base === "en") return base;
  }
  return "en";
}

function systemLanguages(): readonly string[] {
  if (typeof navigator === "undefined") return [];
  return navigator.languages?.length ? navigator.languages : [navigator.language];
}

function readPreference(): LanguagePreference {
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    if (stored === "es" || stored === "en") return stored;
  } catch {
    // storage unavailable: follow the system
  }
  return "system";
}

function writePreference(preference: LanguagePreference) {
  try {
    if (preference === "system") window.localStorage.removeItem(STORAGE_KEY);
    else window.localStorage.setItem(STORAGE_KEY, preference);
  } catch {
    // not persisted; still applies for this session
  }
}

const resolve = (preference: LanguagePreference): Language =>
  preference === "system" ? detectLanguage() : preference;

type LanguageState = { preference: LanguagePreference; language: Language };

const initialPreference = readPreference();
export const useLanguageStore = create<LanguageState>()(() => ({
  preference: initialPreference,
  language: resolve(initialPreference),
}));

function applyDocumentLanguage(language: Language) {
  if (typeof document !== "undefined") document.documentElement.lang = language;
}
applyDocumentLanguage(useLanguageStore.getState().language);

export function setLanguagePreference(preference: LanguagePreference) {
  writePreference(preference);
  const language = resolve(preference);
  useLanguageStore.setState({ preference, language });
  applyDocumentLanguage(language);
}

/** Messages for the current language; re-renders the component when it changes. */
export const useT = (): Messages => MESSAGES[useLanguageStore((s) => s.language)];
/** Outside React (mutation callbacks, plain helpers). */
export const getT = (): Messages => MESSAGES[useLanguageStore.getState().language];

export const useLanguage = () => useLanguageStore((s) => s.language);
/** BCP 47 locale for Intl formatting in the current language. */
export const getLocale = () => LOCALES[useLanguageStore.getState().language];

const numberFormats = new Map<string, Intl.NumberFormat>();
/** Cached Intl.NumberFormat for the current language. */
export function numberFormat(options: Intl.NumberFormatOptions = {}): Intl.NumberFormat {
  const locale = getLocale();
  const key = `${locale}|${JSON.stringify(options)}`;
  let nf = numberFormats.get(key);
  if (!nf) {
    nf = new Intl.NumberFormat(locale, options);
    numberFormats.set(key, nf);
  }
  return nf;
}

/** One decimal, as used for sizes, speeds, ratings and percentages. */
export const oneDecimal = () => numberFormat({ minimumFractionDigits: 1, maximumFractionDigits: 1 });
