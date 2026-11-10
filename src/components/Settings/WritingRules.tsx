import { useTranslation } from 'react-i18next'
import { useAppStore, type PolishChineseScript, type PunctuationMode } from '../../stores/appStore'
import { FormField } from './shared/FormField'
import { LogoSelect } from './shared/LogoSelect'

/** Which Chinese script polish writes in, and how much punctuation it adds. */
export function WritingRules() {
  const { t } = useTranslation()
  const script = useAppStore((s) => s.config.polish_chinese_script)
  const punctuation = useAppStore((s) => s.config.punctuation_mode)
  const updateConfig = useAppStore((s) => s.updateConfig)

  return (
    <>
      <FormField label={t('settings.chineseScript')}>
        <LogoSelect
          label={t('settings.chineseScript')}
          value={script}
          options={[
            { value: 'preserve', label: t('settings.chineseScriptPreserve') },
            { value: 'simplified', label: t('settings.chineseScriptSimplified') },
            { value: 'traditional', label: t('settings.chineseScriptTraditional') },
          ]}
          onChange={(value) =>
            updateConfig({ polish_chinese_script: value as PolishChineseScript })
          }
        />
      </FormField>
      <FormField label={t('settings.punctuationMode')}>
        <LogoSelect
          label={t('settings.punctuationMode')}
          value={punctuation ?? 'auto'}
          options={[
            { value: 'auto', label: t('settings.punctuationAuto') },
            { value: 'spoken', label: t('settings.punctuationSpoken') },
            { value: 'none', label: t('settings.punctuationNone') },
          ]}
          onChange={(value) => updateConfig({ punctuation_mode: value as PunctuationMode })}
        />
      </FormField>
    </>
  )
}
