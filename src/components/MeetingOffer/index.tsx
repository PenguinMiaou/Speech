import { useEffect, useState } from 'react'
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
    return () => clearTimeout(timer)
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
      className="flex h-full w-full items-center gap-3 rounded-[12px] border border-border bg-bg-primary/85 px-4 text-text-primary"
    >
      <span className="flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-full bg-accent text-white">
        <Mic size={17} />
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-[13px] font-semibold">{t(title, { name: found.name })}</p>
        <p className="mt-0.5 truncate text-[12px] text-text-secondary">{t('meeting.offerAsk')}</p>
        <div className="mt-2 flex gap-2">
          <button
            type="button"
            onClick={() => answer(true)}
            className="h-7 rounded-[6px] bg-accent px-3 text-[12px] font-medium text-white hover:bg-accent-hover"
          >
            {t('meeting.offerRecord')}
          </button>
          <button
            type="button"
            onClick={() => answer(false)}
            className="h-7 rounded-[6px] border border-border bg-bg-secondary px-3 text-[12px] text-text-primary hover:bg-bg-tertiary"
          >
            {t('meeting.offerIgnore')}
          </button>
        </div>
      </div>
      <button
        type="button"
        onClick={() => answer(false)}
        aria-label={t('meeting.offerIgnore')}
        className="self-start mt-3 rounded-[6px] p-1 text-text-tertiary hover:bg-bg-secondary hover:text-text-primary"
      >
        <X size={14} />
      </button>
    </div>
  )
}
