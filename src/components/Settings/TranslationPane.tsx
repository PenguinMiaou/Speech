import { useTranslation } from 'react-i18next'
import { useAppStore } from '../../stores/appStore'
import { Toggle } from './shared/Toggle'
import { TranslationTargets } from './TranslationTargets'

/** Which languages dictation is turned into, by the translate shortcut or every time. */
export function TranslationPane() {
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const { t } = useTranslation()

  return (
    <div className="space-y-4">
      <TranslationTargets
        value={config.translation}
        onChange={(translation) => updateConfig({ translation })}
      />
      <Toggle
        checked={config.translate_enabled}
        onChange={(checked) => updateConfig({ translate_enabled: checked })}
        label={t('settings.translationMode')}
      />
    </div>
  )
}
