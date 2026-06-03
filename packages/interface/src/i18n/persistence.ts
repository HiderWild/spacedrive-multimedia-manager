/**
 * Platform-agnostic persistence adapter for language preference.
 * Each platform (web, tauri, mobile) implements this interface.
 */
export interface LanguagePersistence {
  getLanguage: () => string | null | Promise<string | null>;
  setLanguage: (lang: string) => void | Promise<void>;
}
