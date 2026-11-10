import { invoke } from '@tauri-apps/api/core'
import i18n from '../../i18n'
import { useAppStore } from '../../stores/appStore'
import { UI_LANGUAGES } from '../../lib/constants'

/** The language of the app's own interface. */
export function LanguagePane() {
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)

  const currentLang = config.ui_language || i18n.language || 'en'

  const handleSelectLanguage = (value: string) => {
    i18n.changeLanguage(value)
    localStorage.setItem('ui_language', value)
    updateConfig({ ui_language: value })
    invoke('refresh_tray_labels').catch(() => {})
  }

  return (
    <div className="grid grid-cols-2 gap-3 text-[13px]">
      {UI_LANGUAGES.map((lang) => (
        <button
          key={lang.value}
          type="button"
          aria-pressed={currentLang === lang.value}
          onClick={() => handleSelectLanguage(lang.value)}
          className={`px-4 py-3 rounded-[8px] text-[13px] font-medium border cursor-pointer transition-all ${
            currentLang === lang.value
              ? 'bg-accent/10 border-accent text-accent'
              : 'bg-bg-secondary border-border text-text-primary hover:border-text-tertiary'
          }`}
        >
          {lang.label}
        </button>
      ))}
    </div>
  )
}
