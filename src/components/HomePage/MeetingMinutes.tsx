import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  copyText,
  importMinutesTemplate,
  meetingMinutes,
  minutesTemplates,
  removeMinutesTemplate,
  saveTextFile,
  type MinutesTemplate,
} from '../../lib/tauri'
import { LogoSelect } from '../Settings/shared/LogoSelect'

const BUILT_IN = ''
const KNOWN_ERRORS = [
  'llm_not_configured',
  'template_too_large',
  'template_empty',
  'template_unreadable',
]

/** Turns a meeting's text into minutes after a template, the built-in one or one the
 * user imported. */
export function MeetingMinutes({ transcript }: { transcript: string }) {
  const { t } = useTranslation()
  const [templates, setTemplates] = useState<MinutesTemplate[]>([])
  const [chosen, setChosen] = useState(BUILT_IN)
  const [minutes, setMinutes] = useState<string | null>(null)
  const [working, setWorking] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    void Promise.resolve(minutesTemplates())
      .then((all) => setTemplates(all ?? []))
      .catch(() => {})
  }, [])

  const fail = (reason: unknown, fallback: string) => {
    const code = KNOWN_ERRORS.find((known) => String(reason).includes(known))
    setError(`meeting.error.${code ?? fallback}`)
  }

  const write = () => {
    if (working) return
    const template =
      templates.find((entry) => entry.name === chosen)?.text ?? t('meeting.defaultTemplate')
    setWorking(true)
    setError(null)
    Promise.resolve(meetingMinutes(transcript, template))
      .then((text) => setMinutes(text ?? ''))
      .catch((reason) => fail(reason, 'minutes'))
      .finally(() => setWorking(false))
  }

  const add = () => {
    setError(null)
    Promise.resolve(importMinutesTemplate())
      .then((all) => {
        if (!all) return
        const known = new Set(templates.map((entry) => entry.name))
        setTemplates(all)
        setChosen(all.find((entry) => !known.has(entry.name))?.name ?? all[all.length - 1].name)
      })
      .catch((reason) => fail(reason, 'template_unreadable'))
  }

  const remove = () =>
    void Promise.resolve(removeMinutesTemplate(chosen))
      .then((all) => {
        setTemplates(all ?? [])
        setChosen(BUILT_IN)
      })
      .catch(() => {})

  return (
    <div className="mt-4 border-t border-border pt-4">
      <div className="flex flex-wrap items-end gap-2">
        <div className="w-[190px]">
          <LogoSelect
            compact
            label={t('meeting.template')}
            value={chosen}
            options={[
              { value: BUILT_IN, label: t('meeting.defaultTemplateName') },
              ...templates.map((entry) => ({ value: entry.name, label: entry.name })),
            ]}
            onChange={setChosen}
          />
        </div>
        <button type="button" onClick={add} className="btn-quiet">
          {t('meeting.importTemplate')}
        </button>
        {chosen !== BUILT_IN && (
          <button type="button" onClick={remove} className="btn-quiet">
            {t('meeting.removeTemplate')}
          </button>
        )}
        <button type="button" onClick={write} disabled={working} className="btn-record ml-auto">
          {working ? t('meeting.minutesWorking') : t('meeting.minutes')}
        </button>
      </div>
      <p className="mt-2 text-[12px] text-text-tertiary">{t('meeting.minutesNote')}</p>
      {error && <p className="mt-2 text-[12px] text-error">{t(error)}</p>}
      {minutes !== null && (
        <div className="mt-3 space-y-2">
          <textarea
            readOnly
            value={minutes}
            rows={10}
            aria-label={t('meeting.minutesResult')}
            className="w-full resize-y rounded-[8px] border border-border bg-bg-primary px-3 py-2.5 text-[13px] leading-relaxed text-text-primary outline-none"
          />
          <div className="flex gap-2">
            <button type="button" onClick={() => void copyText(minutes)} className="btn-quiet">
              {t('meeting.copy')}
            </button>
            <button
              type="button"
              onClick={() => void saveTextFile(minutes, t('meeting.minutesFileName'))}
              className="btn-quiet"
            >
              {t('meeting.save')}
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
