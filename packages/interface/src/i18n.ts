import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import Backend from 'i18next-resources-to-backend';
import type { LanguagePersistence } from './i18n/persistence';

interface InitI18nOptions {
  persistence: LanguagePersistence;
  defaultLanguage?: string;
}

let initialized = false;

export function initI18n(options: InitI18nOptions) {
  if (initialized) return i18n;
  initialized = true;

  // Synchronous init with default language; persistence is applied async
  i18n
    .use(initReactI18next)
    .use(Backend((lang: string, ns: string) => import(`./locales/${lang}/${ns}.json`)))
    .init({
      lng: options.defaultLanguage || 'zh',
      fallbackLng: 'en',
      defaultNS: 'common',
      ns: ['common', 'sidebar', 'settings', 'dialog', 'errors', 'explorer', 'overview'],
      interpolation: { escapeValue: false },
      react: { useSuspense: false },
    });

  // Load persisted language preference (may be async on mobile)
  Promise.resolve(options.persistence.getLanguage()).then((savedLang) => {
    if (savedLang && savedLang !== i18n.language) {
      i18n.changeLanguage(savedLang);
    }
  });

  // Persist language on change and update document lang attribute
  i18n.on('languageChanged', (lng) => {
    Promise.resolve(options.persistence.setLanguage(lng));
    if (typeof document !== 'undefined') {
      document.documentElement.lang = lng;
    }
  });

  return i18n;
}

export { i18n };
export type { LanguagePersistence };
