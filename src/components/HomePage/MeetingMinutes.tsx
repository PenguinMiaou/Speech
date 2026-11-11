import { Fragment, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Check, LayoutTemplate, Pencil } from 'lucide-react'
import {
  copyText,
  meetingAsk,
  meetingMinutes,
  minutesTemplates,
  saveTextFile,
  type MinutesTemplate,
} from '../../lib/tauri'
import { BUILT_IN, OWN, own, ownKind, TemplateGallery, templateText } from './TemplateGallery'

const KNOWN_ERRORS = ['llm_not_configured']

/** Turns a meeting's text into minutes after a template, the built-in one or one the
 * user imported. */
export function MeetingMinutes({
  transcript,
  notes,
  minutes,
  onMinutes,
  onEdit,
  onSeek,
}: {
  transcript: string
  notes: string
  minutes: string | null
  /** Called with minutes the model has just written. */
  onMinutes: (minutes: string) => void
  /** Called with the minutes as the user changed them. */
  onEdit: (minutes: string) => void
  /** Given when the meeting's sound can be played: a time in the minutes jumps to it. */
  onSeek?: (seconds: number) => void
}) {
  const { t } = useTranslation()
  const [templates, setTemplates] = useState<MinutesTemplate[]>([])
  const [chosen, setChosen] = useState(BUILT_IN)
  const [working, setWorking] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [question, setQuestion] = useState('')
  const [answer, setAnswer] = useState<string | null>(null)
  const [asking, setAsking] = useState(false)
  const [editing, setEditing] = useState(false)
  const [picking, setPicking] = useState(false)

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
    setPicking(false)
    setWorking(true)
    setError(null)
    Promise.resolve(meetingMinutes(transcript, templateText(t, templates, chosen), notes))
      .then((text) => onMinutes(text ?? ''))
      .catch((reason) => fail(reason, 'minutes'))
      .finally(() => setWorking(false))
  }

  const ask = () => {
    if (asking || !question.trim()) return
    setAsking(true)
    setError(null)
    Promise.resolve(meetingAsk(transcript, question.trim()))
      .then((text) => setAnswer(text ?? ''))
      .catch((reason) => fail(reason, 'ask'))
      .finally(() => setAsking(false))
  }

  const chosenName =
    chosen === BUILT_IN
      ? t('meeting.defaultTemplateName')
      : OWN.some((kind) => own(kind) === chosen)
        ? t(`meeting.${ownKind(chosen)}TemplateName`)
        : chosen

  return (
    <div className="mt-3">
      <div className="flex flex-wrap items-end gap-2">
        <button
          type="button"
          onClick={() => setPicking(true)}
          aria-label={t('meeting.template')}
          title={t('meeting.template')}
          className="btn-quiet flex max-w-[240px] items-center gap-1.5"
        >
          <LayoutTemplate size={14} />
          <span className="truncate">{chosenName}</span>
        </button>
        <button type="button" onClick={write} disabled={working} className="btn-record ml-auto">
          {working ? t('meeting.minutesWorking') : t('meeting.minutes')}
        </button>
      </div>
      <p className="mt-2 text-[12px] text-text-tertiary">{t('meeting.minutesNote')}</p>
      {picking && (
        <TemplateGallery
          templates={templates}
          onTemplates={setTemplates}
          chosen={chosen}
          onChoose={setChosen}
          onClose={() => setPicking(false)}
          confirm={{ label: t('meeting.minutes'), onConfirm: write }}
        />
      )}
      <form
        className="mt-3 flex gap-2"
        onSubmit={(event) => {
          event.preventDefault()
          ask()
        }}
      >
        <input
          value={question}
          onChange={(event) => setQuestion(event.target.value)}
          aria-label={t('meeting.ask')}
          placeholder={t('meeting.ask')}
          className="h-9 min-w-0 flex-1 rounded-[8px] border border-border bg-bg-primary px-3 text-[13px] text-text-primary outline-none placeholder:text-text-tertiary"
        />
        <button type="submit" disabled={asking} className="btn-quiet">
          {asking ? t('meeting.asking') : t('meeting.askButton')}
        </button>
      </form>
      {answer !== null && (
        <p role="status" className="mt-2 whitespace-pre-wrap text-[13px] leading-relaxed">
          {withTimes(answer, onSeek)}
        </p>
      )}
      {error && <p className="mt-2 text-[12px] text-error">{t(error)}</p>}
      {minutes !== null && (
        <div className="mt-3 space-y-2">
          {editing ? (
            <textarea
              autoFocus
              value={minutes}
              onChange={(event) => onEdit(event.target.value)}
              rows={12}
              aria-label={t('meeting.minutesResult')}
              className="w-full resize-y rounded-[8px] border border-border bg-bg-primary px-3 py-2.5 text-[13px] leading-relaxed text-text-primary outline-none"
            />
          ) : (
            <MinutesText text={minutes} onChange={onEdit} onSeek={onSeek} />
          )}
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => setEditing(!editing)}
              aria-label={t('meeting.editMinutes')}
              aria-pressed={editing}
              title={t('meeting.editMinutes')}
              className="btn-quiet px-2.5"
            >
              {editing ? <Check size={15} /> : <Pencil size={15} />}
            </button>
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

const TIME = /\[(\d{1,2}:\d{2}(?::\d{2})?)\]/g

/** Text with each time written as [mm:ss] made a way to that moment of the sound. */
function withTimes(text: string, onSeek?: (seconds: number) => void) {
  if (!onSeek) return text
  const pieces: React.ReactNode[] = []
  let read = 0
  for (const found of text.matchAll(TIME)) {
    const at = found.index ?? 0
    const seconds = found[1].split(':').reduce((total, part) => total * 60 + Number(part), 0)
    pieces.push(
      <Fragment key={at}>
        {text.slice(read, at)}
        <button
          type="button"
          onClick={() => onSeek(seconds)}
          className="cursor-pointer rounded-[4px] border-none bg-bg-secondary px-1 text-[12px] tabular-nums text-text-secondary hover:text-accent"
        >
          {found[1]}
        </button>
      </Fragment>,
    )
    read = at + found[0].length
  }
  pieces.push(text.slice(read))
  return pieces
}

/** Text with what stands between two pairs of asterisks made strong, and its times made
 * ways to their moments. */
function rich(text: string, onSeek?: (seconds: number) => void) {
  return text.split(/\*\*(.+?)\*\*/).map((part, index) =>
    index % 2 === 1 ? (
      <strong key={index} className="font-semibold">
        {withTimes(part, onSeek)}
      </strong>
    ) : (
      <Fragment key={index}>{withTimes(part, onSeek)}</Fragment>
    ),
  )
}

/** Minutes laid out to be read: headings stand out, a task is a box to tick, and a time
 * leads to its moment of the sound. Ticking a box changes the text of the minutes. */
function MinutesText({
  text,
  onChange,
  onSeek,
}: {
  text: string
  onChange: (text: string) => void
  onSeek?: (seconds: number) => void
}) {
  const lines = text.split('\n')
  const tick = (index: number) => {
    const next = [...lines]
    next[index] = next[index].replace(
      /^(\s*[-*] \[)([ xX])\]/,
      (_, before: string, mark: string) => `${before}${mark === ' ' ? 'x' : ' '}]`,
    )
    onChange(next.join('\n'))
  }
  return (
    <div className="select-text rounded-[8px] border border-border bg-bg-primary px-3 py-2.5 text-[13px] leading-relaxed text-text-primary">
      {lines.map((line, index) => {
        const task = /^\s*[-*] \[([ xX])\] (.*)$/.exec(line)
        if (task) {
          const done = task[1] !== ' '
          return (
            <label key={index} className="flex items-start gap-2">
              <input
                type="checkbox"
                checked={done}
                onChange={() => tick(index)}
                className="mt-[5px] accent-[var(--color-accent)]"
              />
              <span className={done ? 'text-text-tertiary line-through' : undefined}>
                {rich(task[2], onSeek)}
              </span>
            </label>
          )
        }
        const heading = /^(#{1,4}) (.*)$/.exec(line)
        if (heading) {
          return (
            <p key={index} className={`font-semibold ${index > 0 ? 'mt-2' : ''}`}>
              {withTimes(heading[2], onSeek)}
            </p>
          )
        }
        // A counted or dotted line hangs under its mark.
        const item = /^\s*(?:[-*]|\d+[.)]) /.test(line)
        return (
          <p
            key={index}
            className={`min-h-[1.2em] whitespace-pre-wrap ${item ? 'pl-5 -indent-5' : ''}`}
          >
            {rich(line, onSeek)}
          </p>
        )
      })}
    </div>
  )
}
