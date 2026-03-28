import { createSignal } from "solid-js";
import * as i18n from "@solid-primitives/i18n";
import en from "./en.json";
import zh from "./zh.json";

const dictionaries = { en, zh };
type Locale = keyof typeof dictionaries;

function detectLocale(): Locale {
  const lang = navigator.language.toLowerCase();
  if (lang.startsWith("zh")) return "zh";
  return "en";
}

const [locale, setLocale] = createSignal<Locale>(detectLocale());

type FlatDict = ReturnType<typeof i18n.flatten<typeof en>>;
type TranslationKey = keyof FlatDict;

export function useI18n() {
  const dict = () => i18n.flatten(dictionaries[locale()]);
  const translator = i18n.translator(dict);
  // Allow dynamic string keys without `as any` at call sites
  const t = (key: TranslationKey | (string & {})): string =>
    String(translator(key as TranslationKey) ?? key);
  return { t, locale, setLocale };
}

export { locale, setLocale };
export type { Locale };
