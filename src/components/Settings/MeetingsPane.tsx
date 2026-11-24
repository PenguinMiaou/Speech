import { useTranslation } from 'react-i18next'
import { useAppStore } from '../../stores/appStore'
import { Toggle } from './shared/Toggle'
import { LlmConnectionForm } from './LlmConnectionForm'
import { LogoSelect } from './shared/LogoSelect'

/** Meetings: being asked to record when one starts, and the model that writes the minutes. */
export function MeetingsPane() {
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const { t } = useTranslation()

  return (
    <div className="space-y-4">
      <Toggle
        checked={config.meeting_detect}
        onChange={(checked) => updateConfig({ meeting_detect: checked })}
        label={t('settings.meetingDetect')}
      />
      <div className="flex items-center justify-between gap-3">
        <span className="text-[13px] text-text-primary">{t('settings.meetingQuietPause')}</span>
        <div className="w-44 shrink-0">
          <LogoSelect
            compact
            label={t('settings.meetingQuietPause')}
            value={String(config.meeting_quiet_pause_minutes)}
            options={[5, 10, 15, 30, 0].map((minutes) => ({
              value: String(minutes),
              label:
                minutes === 0
                  ? t('settings.meetingQuietPauseNever')
                  : t('settings.meetingQuietPauseMinutes', { count: minutes }),
            }))}
            onChange={(value) => updateConfig({ meeting_quiet_pause_minutes: Number(value) })}
          />
        </div>
      </div>
      <section
        aria-label={t('settings.meetingModel')}
        className="space-y-3 border-t border-border pt-4"
      >
        <Toggle
          checked={Boolean(config.meeting_llm_provider)}
          onChange={(checked) =>
            updateConfig(
              checked
                ? {
                    meeting_llm_provider: config.llm_provider,
                    meeting_llm_base_url: config.llm_base_url,
                    meeting_llm_api_format: config.llm_api_format,
                    meeting_llm_model: config.llm_model,
                  }
                : {
                    meeting_llm_provider: '',
                    meeting_llm_base_url: '',
                    meeting_llm_api_format: '',
                    meeting_llm_model: '',
                  },
            )
          }
          label={t('settings.meetingModel')}
        />
        <p className="text-[11px] text-text-tertiary">{t('settings.meetingModelHint')}</p>
        {config.meeting_llm_provider && <LlmConnectionForm meeting />}
      </section>
    </div>
  )
}
