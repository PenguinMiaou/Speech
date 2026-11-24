import { useEffect, useId, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Mic, X } from 'lucide-react'
import {
  meetingOfferCurrent,
  meetingOfferDismiss,
  meetingOfferTake,
  onMeetingDetected,
  type MeetingApp,
} from '../../lib/tauri'

/** How long an offer nobody answers stays, in milliseconds. */
const SHOWN_FOR_MS = 60_000

/** The question asked when a meeting or a call is found going on: record it or not.
 * Nothing is recorded until the user says so. */
export function MeetingOffer() {
  const { t } = useTranslation()
  const [found, setFound] = useState<MeetingApp | null>(null)
  const bar = useRef<HTMLDivElement>(null)
  const noteId = useId()

  useEffect(() => {
    void Promise.resolve(meetingOfferCurrent())
      .then((now) => now && setFound(now))
      .catch(() => {})
    const stop = Promise.resolve(onMeetingDetected(setFound))
    return () => void stop.then((unlisten) => unlisten?.())
  }, [])

  useEffect(() => {
    if (!found) return
    const timer = setTimeout(() => {
      setFound(null)
      void Promise.resolve(meetingOfferDismiss()).catch(() => {})
    }, SHOWN_FOR_MS)
    // A thin line along the bottom shrinks over the same time, so the user sees the offer
    // will go by itself.
    const shrink = bar.current?.animate?.(
      [{ transform: 'scaleX(1)' }, { transform: 'scaleX(0)' }],
      {
        duration: SHOWN_FOR_MS,
        easing: 'linear',
        fill: 'forwards',
      },
    )
    return () => {
      clearTimeout(timer)
      shrink?.cancel()
    }
  }, [found])

  if (!found) return null
  const answer = (take: boolean) => {
    setFound(null)
    void Promise.resolve(take ? meetingOfferTake() : meetingOfferDismiss()).catch(() => {})
  }
  const title = found.browser
    ? 'meeting.offerMicrophone'
    : found.call
      ? 'meeting.offerCall'
      : 'meeting.offerMeeting'

  return (
    <div
      role="alertdialog"
      aria-label={t(title, { name: found.name })}
      aria-describedby={noteId}
      className="relative flex h-full w-full items-center gap-3.5 overflow-hidden rounded-lg border border-border bg-bg-primary/85 px-5 text-text-primary"
    >
      <span id={noteId} className="sr-only">
        {t('offer.closesByItself')}
      </span>
      <div
        ref={bar}
        data-testid="offer-countdown"
        aria-hidden="true"
        className="pointer-events-none absolute inset-x-0 bottom-0 h-[2px] origin-left bg-accent/50"
      />
      <span className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-full bg-accent text-white">
        <Mic size={19} />
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-[14px] font-semibold">{t(title, { name: found.name })}</p>
        <p className="mt-0.5 truncate text-[13px] text-text-secondary">{t('meeting.offerAsk')}</p>
        <div className="mt-2.5 flex gap-2">
          <button
            type="button"
            onClick={() => answer(true)}
            className="h-8 rounded-sm bg-accent px-4 text-[13px] font-medium text-white hover:bg-accent-hover"
          >
            {t('meeting.offerRecord')}
          </button>
          <button
            type="button"
            onClick={() => answer(false)}
            className="h-8 rounded-sm border border-border bg-bg-secondary px-4 text-[13px] text-text-primary hover:bg-bg-tertiary"
          >
            {t('meeting.offerIgnore')}
          </button>
        </div>
      </div>
      <button
        type="button"
        onClick={() => answer(false)}
        aria-label={t('meeting.offerIgnore')}
        className="self-start mt-3 rounded-sm p-1 text-text-tertiary hover:bg-bg-secondary hover:text-text-primary"
      >
        <X size={15} />
      </button>
    </div>
  )
}
