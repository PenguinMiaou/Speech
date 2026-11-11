import { useTranslation } from 'react-i18next'
import { Languages, X } from 'lucide-react'
import { TARGET_LANGUAGES } from '../../lib/constants'

/** The most languages a meeting can be said to be held in. */
export const MOST_LANGUAGES = 3

interface Props {
  chosen: string[]
  onChange: (chosen: string[]) => void
}

/** The languages a meeting is held in, as capsules: up to three, none to let the model
 * tell. They can be changed while the meeting is being recorded. */
export function MeetingLanguages({ chosen, onChange }: Props) {
  const { t } = useTranslation()
  const name = (code: string) =>
    TARGET_LANGUAGES.find((language) => language.value === code)?.label ?? code
  const capsule =
    'flex h-7 items-center gap-1 rounded-full border border-border bg-transparent px-2.5 text-[12px] text-text-primary'
  return (
    <div
      role="group"
      aria-label={t('meeting.language')}
      title={t('meeting.language')}
      className="flex flex-wrap items-center gap-1.5"
    >
      <Languages size={15} className="text-text-tertiary" aria-hidden />
      {chosen.length === 0 && (
        <span className="text-[12px] text-text-secondary">{t('meeting.languageAuto')}</span>
      )}
      {chosen.map((code) => (
        <button
          key={code}
          type="button"
          onClick={() => onChange(chosen.filter((each) => each !== code))}
          aria-label={t('meeting.removeLanguage', { name: name(code) })}
          className={`${capsule} cursor-pointer hover:border-text-tertiary`}
        >
          {name(code)}
          <X size={12} className="text-text-tertiary" aria-hidden />
        </button>
      ))}
      {chosen.length < MOST_LANGUAGES && (
        <select
          id="meeting-add-language"
          aria-label={t('meeting.addLanguage')}
          title={t('meeting.addLanguage')}
          value=""
          onChange={(event) => event.target.value && onChange([...chosen, event.target.value])}
          className={`${capsule} w-[44px] cursor-pointer appearance-none text-center text-text-secondary outline-none`}
        >
          <option value="">+</option>
          {TARGET_LANGUAGES.filter((language) => !chosen.includes(language.value)).map(
            (language) => (
              <option key={language.value} value={language.value}>
                {language.label}
              </option>
            ),
          )}
        </select>
      )}
    </div>
  )
}
