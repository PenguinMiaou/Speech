import { Fragment, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { AudioLines, ChevronRight } from 'lucide-react'
import { llmServiceName } from '../../lib/llm-endpoints'
import { STT_PROVIDERS } from '../../lib/constants'
import { meetingsList, type MeetingSummary } from '../../lib/tauri'
import { hotkeyFromBinding, useAppStore, type ShortcutBinding } from '../../stores/appStore'
import { dictationStats, formatMinutes } from '../../lib/dictation-stats'
import { TokenUsageCard } from './TokenUsageCard'

const HOTKEY_SLOT = '\u0001'
const RECENT_MEETINGS = 3

const OUTPUT_MODE_LABELS: Record<string, string> = {
  keyboard: 'settings.keyboardSimulation',
  clipboard: 'settings.clipboardPaste',
}

/** One shortcut as a row of keycaps. */
function Keycaps({ hotkey }: { hotkey: string }) {
  return (
    <span className="inline-flex items-center gap-1">
      {hotkey
        .split('+')
        .filter(Boolean)
        .map((key) => (
          <kbd key={key} className="keycap">
            {key}
          </kbd>
        ))}
    </span>
  )
}

function openMeetings() {
  window.location.hash = '#/meeting'
}

export function HomePage() {
  const config = useAppStore((s) => s.config)
  const history = useAppStore((s) => s.history)
  const { t, i18n } = useTranslation()
  const [meetings, setMeetings] = useState<MeetingSummary[] | null>(null)

  useEffect(() => {
    let live = true
    void Promise.resolve(meetingsList())
      .then((all) => {
        if (!live) return
        const latest = [...(all ?? [])]
          .sort((a, b) => b.started_at - a.started_at)
          .slice(0, RECENT_MEETINGS)
        setMeetings(latest)
      })
      .catch(() => live && setMeetings([]))
    return () => {
      live = false
    }
  }, [])

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

  const sttLabelKey = STT_PROVIDERS.find((p) => p.value === config.stt_provider)?.labelKey
  const outputLabelKey = OUTPUT_MODE_LABELS[config.output_mode]
  const setup = [
    ['home.sttProvider', sttLabelKey ? t(sttLabelKey) : config.stt_provider],
    [
      'home.aiPolish',
      config.polish_enabled
        ? llmServiceName(config.llm_provider, config.llm_base_url)
        : t('home.disabled'),
    ],
    ['home.outputMode', outputLabelKey ? t(outputLabelKey) : config.output_mode],
  ] as const

  // Every shortcut for dictation, Ask and translation, as set in Settings.
  const hotkeys = config.hotkeys
  const bindingsOf = (list: ShortcutBinding[] | undefined, single: ShortcutBinding | null) =>
    (list?.length ? list : single ? [single] : []).map(hotkeyFromBinding)
  const shortcuts = [
    [
      'settings.dictationHotkey',
      hotkeys ? bindingsOf(hotkeys.dictationBindings, hotkeys.dictation) : [config.hotkey],
    ],
    ['settings.askHotkey', hotkeys ? bindingsOf(hotkeys.askBindings, hotkeys.ask) : []],
    [
      'settings.translateHotkey',
      hotkeys ? bindingsOf(hotkeys.translateBindings, hotkeys.translate) : [],
    ],
  ] as const

  const when = (time: number) => {
    try {
      return new Intl.DateTimeFormat(i18n.language, {
        dateStyle: 'medium',
        timeStyle: 'short',
      }).format(new Date(time))
    } catch {
      return new Date(time).toLocaleString()
    }
  }

  return (
    <div className="mx-auto flex w-full max-w-[820px] flex-col gap-5 px-8 pb-8 pt-7">
      <header>
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
        <dl
          aria-label={t('home.shortcuts')}
          className="mt-3 flex flex-wrap gap-x-6 gap-y-2 text-[13px]"
        >
          {shortcuts.map(([label, keys]) => (
            <div key={label} className="flex items-center gap-2">
              <dt className="text-text-tertiary">{t(label)}</dt>
              <dd className="m-0 flex items-center gap-1.5 text-text-secondary">
                {keys.length === 0
                  ? t('home.disabled')
                  : keys.map((hotkey, index) => (
                      <Fragment key={hotkey}>
                        {index > 0 && <span className="text-text-tertiary">/</span>}
                        <Keycaps hotkey={hotkey} />
                      </Fragment>
                    ))}
              </dd>
            </div>
          ))}
        </dl>
      </header>

      <dl
        role="group"
        aria-label={t('home.figures')}
        className="m-0 grid grid-cols-2 border-y border-border sm:grid-cols-4"
      >
        {figures.map(([label, value], index) => (
          <Fragment key={label}>
            <div className={`py-4 pr-5 ${index > 0 ? 'figure-divided pl-5' : ''}`}>
              <dt className="text-[12px] text-text-secondary">{t(label)}</dt>
              <dd className="m-0 mt-1 text-[22px] font-semibold tabular-nums tracking-tight">
                {value}
              </dd>
            </div>
          </Fragment>
        ))}
      </dl>

      <section aria-labelledby="home-recent-meetings">
        <div className="mb-2 flex items-center justify-between gap-3">
          <h3 id="home-recent-meetings" className="section-label">
            {t('home.recentMeetings')}
          </h3>
          <button
            type="button"
            onClick={openMeetings}
            className="flex items-center gap-1.5 rounded-md border border-border bg-transparent px-3 py-1.5 text-[12px] text-text-secondary cursor-pointer transition-colors hover:text-text-primary hover:bg-bg-secondary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
          >
            <AudioLines size={13} aria-hidden="true" />
            {t('home.startMeeting')}
          </button>
        </div>
        {meetings && meetings.length === 0 && (
          <p className="text-[13px] text-text-tertiary">{t('home.noMeetings')}</p>
        )}
        {meetings && meetings.length > 0 && (
          <ul className="m-0 list-none space-y-0.5 p-0">
            {meetings.map((meeting) => (
              <li key={meeting.id}>
                <button
                  type="button"
                  onClick={openMeetings}
                  className="group flex w-full items-center gap-3 rounded-md border-none bg-transparent px-3 py-2.5 text-left cursor-pointer transition-colors hover:bg-bg-secondary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                >
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[13px] text-text-primary">
                      {meeting.title?.trim() || meeting.preview.trim() || t('home.untitledMeeting')}
                    </span>
                    <span className="mt-0.5 block text-[11px] text-text-tertiary">
                      {when(meeting.started_at)}
                      {meeting.duration_s
                        ? ` · ${t('home.meetingMinutes', {
                            count: Math.max(1, Math.round(meeting.duration_s / 60)),
                          })}`
                        : ''}
                    </span>
                  </span>
                  <ChevronRight
                    size={14}
                    aria-hidden="true"
                    className="shrink-0 text-text-tertiary opacity-40 transition-opacity duration-150 group-hover:opacity-100"
                  />
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <TokenUsageCard />
    </div>
  )
}
