import { Fragment } from 'react'
import { useTranslation } from 'react-i18next'
import { llmServiceName } from '../../lib/llm-endpoints'
import { useAppStore } from '../../stores/appStore'
import { dictationStats, formatMinutes } from '../../lib/dictation-stats'
import { MeetingCard } from './MeetingCard'
import { TokenUsageCard } from './TokenUsageCard'

const HOTKEY_SLOT = '\u0001'

export function HomePage() {
  const config = useAppStore((s) => s.config)
  const history = useAppStore((s) => s.history)
  const { t } = useTranslation()

  const now = new Date()
  const today = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`
  const todayCount = history.filter((h) => h.created_at.startsWith(today)).length
  const stats = dictationStats(history)

  const sentence = t('home.speakWith', { hotkey: HOTKEY_SLOT })
  const [before, after = ''] = sentence.includes(HOTKEY_SLOT)
    ? sentence.split(HOTKEY_SLOT)
    : [sentence, '']
  const keys = config.hotkey.split('+').filter(Boolean)

  const figures = [
    ['home.today', String(todayCount)],
    ['home.totalRecordings', history.length.toLocaleString('en-US')],
    ['home.wordsDictated', stats.units.toLocaleString('en-US')],
    ['home.timeSaved', formatMinutes(stats.savedMinutes)],
  ] as const

  const setup = [
    ['home.sttProvider', config.stt_provider],
    [
      'home.aiPolish',
      config.polish_enabled
        ? llmServiceName(config.llm_provider, config.llm_base_url)
        : t('home.disabled'),
    ],
    ['home.outputMode', config.output_mode],
  ] as const

  return (
    <div className="flex max-w-[784px] flex-col gap-5 px-8 pb-8">
      <header className="pt-7">
        <h2 className="page-title flex flex-wrap items-center gap-x-2 gap-y-2">
          {before}
          <span className="inline-flex items-center gap-1.5">
            {keys.map((key) => (
              <kbd key={key} className="keycap">
                {key}
              </kbd>
            ))}
          </span>
          {after}
        </h2>
        <dl className="mt-3 flex flex-wrap gap-x-6 gap-y-1 text-[13px]">
          {setup.map(([label, value]) => (
            <div key={label} className="flex items-baseline gap-2">
              <dt className="text-text-tertiary">{t(label)}</dt>
              <dd className="m-0 text-text-secondary">{value}</dd>
            </div>
          ))}
        </dl>
      </header>

      <dl
        role="group"
        aria-label={t('home.figures')}
        className="surface m-0 grid grid-cols-2 sm:grid-cols-4"
      >
        {figures.map(([label, value], index) => (
          <Fragment key={label}>
            <div className={`px-5 py-4 ${index > 0 ? 'figure-divided' : ''}`}>
              <dt className="text-[12px] text-text-secondary">{t(label)}</dt>
              <dd className="m-0 mt-1 text-[24px] font-semibold tabular-nums tracking-tight">
                {value}
              </dd>
            </div>
          </Fragment>
        ))}
      </dl>

      <MeetingCard />

      <TokenUsageCard />
    </div>
  )
}
