import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { updateConfig as persistConfig, voiceIsolationEnrollNext } from '../../lib/tauri'
import { useAppStore } from '../../stores/appStore'

/** A name for the app. Saying it first turns what follows into an instruction. */
export function NameDemo() {
  const { t } = useTranslation()
  const saved = useAppStore((s) => s.config.assistant_name)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const [name, setName] = useState(saved)

  // Nothing is read aloud here, so there is no voice sample to take.
  useEffect(() => {
    voiceIsolationEnrollNext(false)?.catch(() => {})
  }, [])

  const save = () => {
    const assistant_name = name.trim()
    if (assistant_name === useAppStore.getState().config.assistant_name) return
    updateConfig({ assistant_name })
    persistConfig({ ...useAppStore.getState().config, assistant_name })?.catch(() => {})
  }

  return (
    <div className="space-y-3">
      <input
        type="text"
        value={name}
        onChange={(event) => setName(event.target.value)}
        onBlur={save}
        onKeyDown={(event) => {
          if (event.key === 'Enter') save()
        }}
        aria-label={t('settings.assistantName')}
        placeholder={t('settings.assistantNamePlaceholder')}
        maxLength={24}
        className="w-full rounded-xl border border-border bg-bg-secondary px-5 py-3 text-center text-[15px] text-text-primary outline-none focus:border-border-focus"
      />
      <p className="text-center text-[13px] leading-relaxed text-text-secondary">
        {t('onboarding.tryIt.nameHint', {
          name: name.trim() || t('onboarding.tryIt.nameExample'),
        })}
      </p>
    </div>
  )
}
