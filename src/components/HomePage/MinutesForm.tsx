import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { TARGET_LANGUAGES } from '../../lib/constants'
import {
  minutesFull,
  minutesLanguage,
  setMinutesFull,
  setMinutesLanguage,
} from '../../stores/meetingStore'

/** How the next minutes are written: brief or full, and in which language. Each choice is
 * remembered, and the minutes are written as last chosen. */
export function MinutesForm() {
  const { t } = useTranslation()
  const [full, setFull] = useState(minutesFull)
  const [language, setLanguage] = useState(minutesLanguage)
  return (
    <>
      <div
        role="radiogroup"
        aria-label={t('meeting.length')}
        className="flex rounded-[8px] border border-border p-0.5 text-[12px]"
      >
        {[false, true].map((value) => (
          <button
            key={String(value)}
            type="button"
            role="radio"
            aria-checked={full === value}
            onClick={() => {
              setFull(value)
              setMinutesFull(value)
            }}
            className={`rounded-[6px] px-2 py-1 ${full === value ? 'bg-bg-secondary text-text-primary' : 'text-text-tertiary'}`}
          >
            {t(value ? 'meeting.lengthFull' : 'meeting.lengthBrief')}
          </button>
        ))}
      </div>
      <select
        aria-label={t('meeting.minutesLanguage')}
        title={t('meeting.minutesLanguage')}
        value={language}
        onChange={(event) => {
          setLanguage(event.target.value)
          setMinutesLanguage(event.target.value)
        }}
        className="h-[30px] rounded-[8px] border border-border bg-bg-primary px-2 text-[12px] text-text-primary outline-none"
      >
        <option value="">{t('meeting.minutesLanguageSame')}</option>
        {TARGET_LANGUAGES.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </>
  )
}
