import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Loader2, RotateCw } from 'lucide-react'
import { meetingOverview, type MeetingOverview } from '../../lib/tauri'
import { minutesLanguage } from '../../stores/meetingStore'

interface Props {
  /** The meeting the overview belongs to; null for one not kept yet. */
  meetingId: string | null
  /** What was said, with the speakers' numbers in its headings. */
  transcript: string
  /** What speakers are called in the headings. */
  label: string
  /** The names the user gave the speakers, by number. */
  names: Record<number, string>
  overview: MeetingOverview | null
  /** Whether the overview is on screen: it is asked for only then. */
  active: boolean
  /** Whether the meeting is being recorded, written out or put right, when the text is
   * still changing and no overview is asked for. */
  busy: boolean
  onOverview: (overview: MeetingOverview) => void
  /** Plays the recording from a second, where there is one. */
  onSeek?: (seconds: number) => void
}

function clock(seconds: number): string {
  const two = (value: number) => String(value).padStart(2, '0')
  const rest = `${two(Math.floor(seconds / 60) % 60)}:${two(seconds % 60)}`
  const hours = Math.floor(seconds / 3600)
  return hours > 0 ? `${hours}:${rest}` : rest
}

/** The chapters of a meeting, each from the time it began, and what each speaker said.
 * Asked of the language model the first time it is shown, and again on request. */
export function MeetingOverviewPanel({
  meetingId,
  transcript,
  label,
  names,
  overview,
  active,
  busy,
  onOverview,
  onSeek,
}: Props) {
  const { t } = useTranslation()
  const [working, setWorking] = useState(false)
  const [error, setError] = useState<string | null>(null)
  /** The meeting an overview was last asked for on its own, so that one that failed is not
   * asked for again until the user says so. */
  const asked = useRef<string | null>(null)
  /** The meeting and text as they are now, for an answer that comes after they changed. */
  const latest = useRef({ meetingId, transcript })
  latest.current = { meetingId, transcript }

  const make = () => {
    if (working || busy || !transcript.trim()) return
    const forMeeting = meetingId
    const forText = transcript
    setWorking(true)
    setError(null)
    Promise.resolve(meetingOverview(forText, label, names, minutesLanguage()))
      .then((made) => {
        const now = latest.current
        if (!made || now.meetingId !== forMeeting || now.transcript !== forText) return
        onOverview({ ...made, chars: forText.length })
      })
      .catch((reason) => {
        if (latest.current.meetingId !== forMeeting) return
        setError(
          String(reason).includes('llm_not_configured')
            ? 'meeting.error.llm_not_configured'
            : 'meeting.error.overview',
        )
      })
      .finally(() => setWorking(false))
  }

  useEffect(() => {
    const key = meetingId ?? ''
    if (!active || overview || busy || working || asked.current === key) return
    asked.current = key
    make()
    // `make` reads the current props; the overview is asked for when it comes into view.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, overview, busy, meetingId])

  const who = (speaker: number) => {
    const name = names[speaker]?.trim()
    if (name) return name
    return speaker === 0 ? label : `${label} ${speaker}`
  }
  const stale = overview !== null && overview.chars !== transcript.length

  if (!overview) {
    return (
      <div className="flex flex-col items-start gap-2 py-6 text-[13px] text-text-secondary">
        {working ? (
          <span role="status" className="flex items-center gap-2">
            <Loader2 size={14} className="animate-spin" />
            {t('meeting.overviewMaking')}
          </span>
        ) : error ? (
          <>
            <span role="alert" className="text-error">
              {t(error)}
            </span>
            <button
              type="button"
              onClick={make}
              disabled={busy}
              className="cursor-pointer rounded-md border border-border bg-transparent px-3 py-1.5 text-[12px] text-text-primary hover:border-border-focus disabled:cursor-default disabled:opacity-50"
            >
              {t('meeting.overviewAgain')}
            </button>
          </>
        ) : (
          <button
            type="button"
            onClick={make}
            disabled={busy}
            className="cursor-pointer rounded-md border border-border bg-transparent px-3 py-1.5 text-[12px] text-text-primary hover:border-border-focus disabled:cursor-default disabled:opacity-50"
          >
            {t('meeting.overviewMake')}
          </button>
        )}
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-5">
      <div className="flex items-center justify-between gap-3">
        <span className="text-[12px] text-text-tertiary">
          {stale ? t('meeting.overviewStale') : ''}
        </span>
        <button
          type="button"
          onClick={make}
          disabled={working || busy}
          className="flex cursor-pointer items-center gap-1.5 rounded-md border-none bg-transparent px-2 py-1 text-[12px] text-text-secondary hover:bg-bg-hover hover:text-text-primary disabled:cursor-default disabled:opacity-50"
        >
          {working ? <Loader2 size={13} className="animate-spin" /> : <RotateCw size={13} />}
          {t('meeting.overviewAgain')}
        </button>
      </div>
      {error && (
        <span role="alert" className="text-[12px] text-error">
          {t(error)}
        </span>
      )}

      <section aria-label={t('meeting.chapters')}>
        <h3 className="section-label mb-2">{t('meeting.chapters')}</h3>
        <ol className="m-0 flex list-none flex-col gap-3 p-0">
          {overview.chapters.map((chapter) => (
            <li key={chapter.at_s} className="flex gap-3">
              {onSeek ? (
                <button
                  type="button"
                  onClick={() => onSeek(chapter.at_s)}
                  aria-label={t('meeting.playFrom', { time: clock(chapter.at_s) })}
                  className="h-fit cursor-pointer border-none bg-transparent p-0 text-[12px] tabular-nums text-accent hover:underline"
                >
                  {clock(chapter.at_s)}
                </button>
              ) : (
                <span className="text-[12px] tabular-nums text-text-tertiary">
                  {clock(chapter.at_s)}
                </span>
              )}
              <div className="min-w-0">
                <div className="text-[13px] font-medium text-text-primary">{chapter.title}</div>
                {chapter.summary && (
                  <p className="m-0 mt-0.5 text-[13px] leading-relaxed text-text-secondary">
                    {chapter.summary}
                  </p>
                )}
              </div>
            </li>
          ))}
        </ol>
      </section>

      {overview.speakers.length > 0 && (
        <section aria-label={t('meeting.speakerSummaries')}>
          <h3 className="section-label mb-2">{t('meeting.speakerSummaries')}</h3>
          <ul className="m-0 flex list-none flex-col gap-3 p-0">
            {overview.speakers.map((said) => (
              <li key={said.speaker}>
                <div className="text-[13px] font-medium text-text-primary">{who(said.speaker)}</div>
                <p className="m-0 mt-0.5 text-[13px] leading-relaxed text-text-secondary">
                  {said.summary}
                </p>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  )
}
