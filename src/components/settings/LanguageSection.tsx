import {
  detectLanguage,
  setLanguagePreference,
  useLanguageStore,
  useT,
  type LanguagePreference,
} from "../../i18n";
import { SetRow, SetSection, Segmented } from "./controls";

/** Each language is named in itself, so it can be found whatever the current one is. */
const NATIVE_NAMES = { es: "Español", en: "English" } as const;

/** Ajustes › Idioma: the app's language, or the system's ("Automático"). Stored on this device. */
export function LanguageSection() {
  const t = useT().language;
  const preference = useLanguageStore((s) => s.preference);
  const options: { value: LanguagePreference; label: string }[] = [
    { value: "system", label: t.system },
    { value: "es", label: NATIVE_NAMES.es },
    { value: "en", label: NATIVE_NAMES.en },
  ];
  return (
    <SetSection id="s-lang" title={t.title}>
      <SetRow title={t.app} help={t.help(NATIVE_NAMES[detectLanguage()])}>
        <Segmented label={t.app} value={preference} options={options} onChange={setLanguagePreference} />
      </SetRow>
    </SetSection>
  );
}
