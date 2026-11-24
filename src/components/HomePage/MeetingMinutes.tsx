import { Fragment, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Check, LayoutTemplate, Palette, Pencil, Star, X } from 'lucide-react'
import {
  meetingAsk,
  meetingMinutes,
  meetingQuestions,
  meetingQuestionsSave,
  minutesLearn,
  minutesStyle,
  minutesStyleSave,
  minutesTemplates,
  saveTextFile,
  type MeetingAnswer,
  type MinutesTemplate,
} from '../../lib/tauri'
import { BUILT_IN, OWN, own, ownKind, TemplateGallery, templateText } from './TemplateGallery'
import { minutesFull, minutesLanguage } from '../../stores/meetingStore'
import { CopyButton } from '../CopyButton'
import { toast } from '../toast-service'
import { MinutesForm } from './MinutesForm'

const KNOWN_ERRORS = ['llm_not_configured']
/** The most questions and answers kept with a meeting. */
const MOST_ANSWERS = 20

/** Turns a meeting's text into minutes after a template, the built-in one or one the
 * user imported. */
export function MeetingMinutes({
  transcript,
  notes,
  minutes,
  onMinutes,
  onEdit,
  onSeek,
  answers = [],
  onAnswers,
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
  /** The questions asked of the meeting and their answers, the newest first. */
  answers?: MeetingAnswer[]
  /** Called with the questions and answers as they now stand, to be kept with the meeting. */
  onAnswers?: (answers: MeetingAnswer[]) => void
}) {
  const { t } = useTranslation()
  const [templates, setTemplates] = useState<MinutesTemplate[]>([])
  const [chosen, setChosen] = useState(BUILT_IN)
  const [working, setWorking] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [question, setQuestion] = useState('')
  /** The questions the user keeps to ask of any meeting. */
  const [kept, setKept] = useState<string[]>([])
  /** The meeting and its answers as they are now, for an answer that comes later. */
  const latest = useRef({ transcript, answers, onAnswers })
  latest.current = { transcript, answers, onAnswers }
  const [asking, setAsking] = useState(false)
  const [editing, setEditing] = useState(false)
  const [picking, setPicking] = useState(false)
  /** The rules learned for writing minutes after the template these minutes follow. */
  const [style, setStyle] = useState<string[]>([])
  const [showingStyle, setShowingStyle] = useState(false)
  /** Set while the model learns from the user's change. */
  const [learning, setLearning] = useState(false)
  /** The minutes as they were when the user began to change them. */
  const before = useRef<string | null>(null)
  const firstLine = (minutes ?? '').split('\n').find((line) => line.trim() !== '') ?? ''

  useEffect(() => {
    if (!firstLine) return setStyle([])
    void Promise.resolve(minutesStyle(firstLine))
      .then((rules) => setStyle(rules ?? []))
      .catch(() => {})
  }, [firstLine])

  useEffect(() => {
    void Promise.resolve(meetingQuestions())
      .then((all) => setKept(all ?? []))
      .catch(() => {})
  }, [])

  /** Begins or ends a change by hand; an ended change teaches how the user wants minutes. */
  const toggleEditing = () => {
    if (!editing) {
      before.current = minutes
      setEditing(true)
      return
    }
    setEditing(false)
    const given = before.current
    before.current = null
    if (given === null || minutes === null || given === minutes) return
    setLearning(true)
    void Promise.resolve(minutesLearn(given, minutes))
      .then((rules) => rules && setStyle(rules))
      .catch(() => {})
      .finally(() => setLearning(false))
  }

  const forget = (rule: string) => {
    const rules = style.filter((other) => other !== rule)
    setStyle(rules)
    void Promise.resolve(minutesStyleSave(firstLine, rules)).catch(() =>
      toast.error(t('meeting.error.style')),
    )
  }

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
    Promise.resolve(
      meetingMinutes(
        transcript,
        templateText(t, templates, chosen),
        notes,
        minutesFull(),
        minutesLanguage(),
      ),
    )
      .then((text) => onMinutes(text ?? ''))
      .catch((reason) => fail(reason, 'minutes'))
      .finally(() => setWorking(false))
  }

  const ask = (asked = question) => {
    const plain = asked.trim()
    if (asking || !plain) return
    setQuestion(plain)
    setAsking(true)
    setError(null)
    // The answer belongs to the meeting it was asked of: it is dropped if another meeting
    // is open by the time it comes, and goes before the answers as they then stand.
    const askedOf = transcript
    Promise.resolve(meetingAsk(transcript, plain))
      .then((text) => {
        if (latest.current.transcript !== askedOf) return
        latest.current.onAnswers?.(
          [
            { question: plain, answer: text ?? '', at: Date.now() },
            ...latest.current.answers,
          ].slice(0, MOST_ANSWERS),
        )
      })
      .catch((reason) => fail(reason, 'ask'))
      .finally(() => setAsking(false))
  }
  const keepQuestions = (next: string[]) => {
    setKept(next)
    void Promise.resolve(meetingQuestionsSave(next))
      .then((now) => now && setKept(now))
      .catch(() => toast.error(t('meeting.error.questions')))
  }
  /** The questions that come with the app, then the user's own. */
  const ready = [
    t('meeting.questionFollowUp'),
    t('meeting.questionRisks'),
    t('meeting.questionBrief'),
  ]

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
        <MinutesForm />
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
          className="h-9 min-w-0 flex-1 rounded-md border border-border bg-bg-primary px-3 text-[13px] text-text-primary outline-none placeholder:text-text-tertiary"
        />
        <button type="submit" disabled={asking} className="btn-quiet">
          {asking ? t('meeting.asking') : t('meeting.askButton')}
        </button>
      </form>
      <div
        role="group"
        aria-label={t('meeting.questionsKept')}
        className="mt-2 flex flex-wrap gap-1.5 text-[12px]"
      >
        {ready.map((each) => (
          <button
            key={each}
            type="button"
            disabled={asking}
            onClick={() => ask(each)}
            className="rounded-full border border-border px-2.5 py-1 text-text-secondary hover:bg-bg-secondary"
          >
            {each}
          </button>
        ))}
        {kept.map((each) => (
          <span
            key={each}
            className="inline-flex items-center rounded-full border border-border text-text-secondary"
          >
            <button
              type="button"
              disabled={asking}
              onClick={() => ask(each)}
              className="max-w-[260px] truncate rounded-l-full py-1 pl-2.5 pr-1 hover:bg-bg-secondary"
            >
              {each}
            </button>
            <button
              type="button"
              onClick={() => keepQuestions(kept.filter((other) => other !== each))}
              aria-label={t('meeting.questionForget', { question: each })}
              title={t('meeting.questionForget', { question: each })}
              className="rounded-r-full py-1 pl-0.5 pr-2 hover:text-error"
            >
              <X size={11} />
            </button>
          </span>
        ))}
        {question.trim() !== '' &&
          !kept.includes(question.trim()) &&
          !ready.includes(question.trim()) && (
            <button
              type="button"
              onClick={() => keepQuestions([...kept, question.trim()])}
              className="inline-flex items-center gap-1 rounded-full px-2 py-1 text-text-tertiary hover:text-accent"
            >
              <Star size={11} />
              {t('meeting.questionKeep')}
            </button>
          )}
      </div>
      {answers.length > 0 && (
        <ul aria-label={t('meeting.answers')} className="mt-2 space-y-2">
          {answers.map((each, index) => (
            <li
              key={`${each.at}-${index}`}
              className="rounded-md border border-border px-3 py-2 text-[13px] leading-relaxed"
            >
              <div className="flex items-start gap-2">
                <p className="min-w-0 flex-1 font-medium text-text-secondary">{each.question}</p>
                <CopyButton
                  text={each.answer}
                  label={t('meeting.copy')}
                  size={12}
                  className="btn-quiet px-1"
                />
                <button
                  type="button"
                  onClick={() => onAnswers?.(answers.filter((_, at) => at !== index))}
                  aria-label={t('meeting.answerForget')}
                  title={t('meeting.answerForget')}
                  className="btn-quiet px-1"
                >
                  <X size={12} />
                </button>
              </div>
              <p role={index === 0 ? 'status' : undefined} className="mt-1 whitespace-pre-wrap">
                {withTimes(each.answer, onSeek)}
              </p>
            </li>
          ))}
        </ul>
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
              className="w-full resize-y rounded-md border border-border bg-bg-primary px-3 py-2.5 text-[13px] leading-relaxed text-text-primary outline-none"
            />
          ) : (
            <MinutesText text={minutes} onChange={onEdit} onSeek={onSeek} />
          )}
          <div className="flex gap-2">
            <button
              type="button"
              onClick={toggleEditing}
              aria-label={t('meeting.editMinutes')}
              aria-pressed={editing}
              title={t('meeting.editMinutes')}
              className="btn-quiet px-2.5"
            >
              {editing ? <Check size={15} /> : <Pencil size={15} />}
            </button>
            <CopyButton text={minutes} label={t('meeting.copy')} size={13} className="btn-quiet">
              {t('meeting.copy')}
            </CopyButton>
            <button
              type="button"
              onClick={() => void saveTextFile(minutes, t('meeting.minutesFileName'))}
              className="btn-quiet"
            >
              {t('meeting.save')}
            </button>
            {(style.length > 0 || learning) && (
              <button
                type="button"
                onClick={() => setShowingStyle(!showingStyle)}
                aria-expanded={showingStyle}
                className={`btn-quiet ml-auto flex items-center gap-1 ${learning ? 'animate-pulse' : ''}`}
              >
                <Palette size={14} />
                {learning
                  ? t('meeting.styleLearning')
                  : t('meeting.style', { count: style.length })}
              </button>
            )}
          </div>
          {showingStyle && style.length > 0 && (
            <div className="rounded-md border border-border px-3 py-2 text-[12px]">
              <p className="text-text-tertiary">{t('meeting.styleNote')}</p>
              <ul
                aria-label={t('meeting.style', { count: style.length })}
                className="mt-1.5 space-y-1"
              >
                {style.map((rule) => (
                  <li key={rule} className="flex items-start gap-2 text-text-secondary">
                    <span className="min-w-0 flex-1">{rule}</span>
                    <button
                      type="button"
                      onClick={() => forget(rule)}
                      aria-label={t('meeting.styleForget', { rule })}
                      title={t('meeting.styleForget', { rule })}
                      className="btn-quiet px-1"
                    >
                      <X size={12} />
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          )}
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
          className="cursor-pointer rounded-xs border-none bg-bg-secondary px-1 text-[12px] tabular-nums text-text-secondary hover:text-accent"
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
    <div className="select-text rounded-md border border-border bg-bg-primary px-3 py-2.5 text-[13px] leading-relaxed text-text-primary">
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
