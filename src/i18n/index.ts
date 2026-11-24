import i18n, { type BackendModule } from 'i18next'
import { initReactI18next } from 'react-i18next'
import en from './locales/en.json'

/** Every language but English is read when it is first wanted, so a window carries only the
 * one it shows; English is always there to fall back on. */
const others = import.meta.glob<{ default: Record<string, unknown> }>([
  './locales/*.json',
  '!./locales/en.json',
])

const lazyLanguages: BackendModule = {
  type: 'backend',
  init() {},
  read(language, _namespace, done) {
    const load = others[`./locales/${language}.json`]
    if (!load) return done(null, {})
    load().then(
      (module) => done(null, module.default),
      (error: Error) => done(error, null),
    )
  },
}

const savedLang =
  typeof localStorage !== 'undefined' ? localStorage.getItem('ui_language') || 'en' : 'en'

/** Settles once the language to show is read, so the first frame is already in it. */
export const i18nReady = i18n
  .use(lazyLanguages)
  .use(initReactI18next)
  .init({
    resources: { en: { translation: en } },
    partialBundledLanguages: true,
    lng: savedLang,
    fallbackLng: 'en',
    supportedLngs: ['en', 'zh', 'ja', 'ko', 'fr', 'de', 'es', 'pt', 'ru', 'it'],
    interpolation: { escapeValue: false },
  })

export default i18n
