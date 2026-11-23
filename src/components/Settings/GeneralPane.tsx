import { useTranslation } from 'react-i18next'
import { useAppStore } from '../../stores/appStore'
import { Toggle } from './shared/Toggle'
import { AdvancedExtras } from './AdvancedExtras'
import { LanguagePane } from './LanguagePane'

/** The app itself: its language, how it starts and shows, and what it keeps. */
export function GeneralPane() {
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const { t } = useTranslation()

  return (
    <div className="space-y-6">
      <Section title={t('settings.language')}>
        <LanguagePane />
      </Section>

      <Section title={t('settings.other')}>
        <div className="space-y-3">
          <Toggle
            checked={config.auto_start}
            onChange={(checked) => updateConfig({ auto_start: checked })}
            label={t('settings.launchAtStartup')}
          />
          <Toggle
            checked={config.start_minimized}
            onChange={(checked) => updateConfig({ start_minimized: checked })}
            label={t('settings.startMinimized')}
          />
          <Toggle
            checked={config.capsule_auto_hide}
            onChange={(checked) => updateConfig({ capsule_auto_hide: checked })}
            label={t('settings.hideCapsuleWhenIdle')}
          />
        </div>
      </Section>

      <Section title={t('settings.historyPrivacy')}>
        <div className="space-y-3">
          <Toggle
            checked={config.history_enabled}
            onChange={(checked) => updateConfig({ history_enabled: checked })}
            label={t('settings.saveHistory')}
          />
          <AdvancedExtras part="history" />
        </div>
      </Section>
    </div>
  )
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <h3 className="section-label mb-2.5">{title}</h3>
      {children}
    </div>
  )
}
