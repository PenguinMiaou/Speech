import { Fragment } from 'react'
import { useTranslation } from 'react-i18next'
import { AudioLines, ChevronRight, MessageCircleQuestion } from 'lucide-react'
import { hotkeyFromBinding, useAppStore } from '../../stores/appStore'

/** The last step: what Speech does besides dictation, meeting recording and the Ask panel. */
export function MoreStep() {
  const { t } = useTranslation()
  const config = useAppStore((s) => s.config)

  const askBindings = config.hotkeys?.askBindings?.length
    ? config.hotkeys.askBindings
    : config.hotkeys?.ask
      ? [config.hotkeys.ask]
      : []
  const askHotkey = askBindings.length ? hotkeyFromBinding(askBindings[0]) : config.ask_hotkey
  const askKeys = (askHotkey ?? '').split('+').filter(Boolean)

  const stages = ['onboarding.more.record', 'onboarding.more.transcript', 'onboarding.more.minutes']

  return (
    <div className="space-y-3">
      <section className="rounded-md bg-bg-secondary px-4 py-3.5">
        <div className="flex items-center gap-3">
          <div className="grid h-7 w-7 shrink-0 place-items-center rounded-md bg-bg-tertiary text-text-tertiary">
            <AudioLines size={14} />
          </div>
          <h3 className="text-[13px] font-medium text-text-primary">
            {t('onboarding.more.meetingTitle')}
          </h3>
        </div>
        <ol
          aria-label={t('onboarding.more.meetingTitle')}
          className="m-0 mt-3 flex list-none flex-wrap items-center gap-1.5 p-0"
        >
          {stages.map((key, index) => (
            <Fragment key={key}>
              {index > 0 && (
                <ChevronRight size={12} aria-hidden="true" className="text-text-tertiary" />
              )}
              <li className="rounded-full bg-bg-primary px-2.5 py-1 text-[11px] font-medium text-text-secondary">
                {t(key)}
              </li>
            </Fragment>
          ))}
        </ol>
        <p className="mt-2.5 text-[12px] leading-relaxed text-text-secondary">
          {t('onboarding.more.meetingDesc')}
        </p>
      </section>

      <section className="rounded-md bg-bg-secondary px-4 py-3.5">
        <div className="flex items-center gap-3">
          <div className="grid h-7 w-7 shrink-0 place-items-center rounded-md bg-bg-tertiary text-text-tertiary">
            <MessageCircleQuestion size={14} />
          </div>
          <h3 className="flex-1 text-[13px] font-medium text-text-primary">
            {t('onboarding.more.askTitle')}
          </h3>
          {askKeys.length > 0 && (
            <span data-testid="ask-hotkey" className="inline-flex items-center gap-1">
              {askKeys.map((key) => (
                <kbd key={key} className="keycap">
                  {key}
                </kbd>
              ))}
            </span>
          )}
        </div>
        <p className="mt-2.5 text-[12px] leading-relaxed text-text-secondary">
          {t('onboarding.more.askDesc')}
        </p>
        {askKeys.length === 0 && (
          <p className="mt-1.5 text-[12px] text-text-tertiary">
            {t('onboarding.more.askNoHotkey')}
          </p>
        )}
      </section>
    </div>
  )
}
