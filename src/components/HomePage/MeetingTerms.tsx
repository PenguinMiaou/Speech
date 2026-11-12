import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { BookOpen, Check, Play, SpellCheck, X } from 'lucide-react'
import type { MeetingTerm } from '../../lib/tauri'
import { whereSaid, wordsLike, type Times } from '../../lib/meeting-text'
import { useAppStore } from '../../stores/appStore'

/** The most ways of writing a term that are offered to pick from. */
const OFFERED = 3

interface Props {
  terms: MeetingTerm[]
  /** The meeting's text, to show where each term was said. */
  text: string
  speakerLabel: string
  times: Times[]
  /** Plays the recording from a time in seconds; absent where no recording is kept. */
  onHear?: (seconds: number) => void
  onSettle: (spellings: string[]) => void
  onSkip: () => void
}

/** Asks how the terms a model was unsure of are written: each with the ways it was
 * written to pick from, a field to type another, and the place it was said. */
export function MeetingTerms({
  terms,
  text,
  speakerLabel,
  times,
  onHear,
  onSettle,
  onSkip,
}: Props) {
  const { t } = useTranslation()
  const dictionary = useAppStore((state) => state.dictionary)
  const known = dictionary.map((entry) => entry.word)
  const [spellings, setSpellings] = useState(() => terms.map((term) => term.spelling))
  const write = (index: number, spelling: string) =>
    setSpellings((now) => now.map((had, at) => (at === index ? spelling : had)))
  return (
    <form
      aria-label={t('meeting.terms')}
      onSubmit={(event) => {
        event.preventDefault()
        onSettle(spellings)
      }}
      className="flex flex-col gap-2.5 rounded-[10px] border border-border bg-bg-secondary px-3 py-2.5"
    >
      <div className="flex items-center gap-2 text-[12px] text-text-secondary">
        <SpellCheck size={13} />
        <span className="font-medium text-text-primary">{t('meeting.terms')}</span>
        <span className="min-w-0 flex-1 truncate text-text-tertiary">{t('meeting.termsHint')}</span>
        <button
          type="submit"
          aria-label={t('meeting.termsApply')}
          className="flex h-6 cursor-pointer items-center gap-1 rounded-[6px] border-none bg-accent px-2 text-[12px] text-white"
        >
          <Check size={13} />
          <span aria-hidden>{t('meeting.termsApply')}</span>
        </button>
        <button
          type="button"
          aria-label={t('meeting.termsSkip')}
          title={t('meeting.termsSkip')}
          onClick={onSkip}
          className="flex h-6 w-6 cursor-pointer items-center justify-center rounded-[6px] border-none bg-transparent text-text-tertiary hover:text-text-primary"
        >
          <X size={13} />
        </button>
      </div>
      <ul className="m-0 flex list-none flex-col gap-2.5 p-0">
        {terms.map((term, index) => {
          // The user's own words come before the other ways the meeting writes the term.
          const own = wordsLike([term.spelling, ...term.others], known)
          const ways = [...new Set([term.spelling, ...own, ...term.others])].slice(0, OFFERED)
          const said = whereSaid(text, speakerLabel, [term.spelling, ...term.others], times)
          const seconds = said?.seconds ?? null
          return (
            <li key={term.spelling} className="flex min-w-0 flex-col gap-1">
              <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                <input
                  value={spellings[index] ?? ''}
                  onChange={(event) => write(index, event.target.value)}
                  aria-label={term.others.join(', ')}
                  className="h-7 w-[170px] shrink-0 rounded-[7px] border border-border bg-bg-primary px-2 text-[12px] font-medium outline-none focus:border-accent"
                />
                {ways.map((way) => (
                  <button
                    key={way}
                    type="button"
                    aria-pressed={spellings[index] === way}
                    onClick={() => write(index, way)}
                    className="flex h-7 cursor-pointer items-center gap-1 rounded-[7px] border border-border bg-transparent px-2 text-[12px] text-text-secondary hover:text-text-primary aria-pressed:border-accent aria-pressed:text-accent"
                  >
                    {own.includes(way) && <BookOpen size={11} aria-hidden />}
                    {way}
                  </button>
                ))}
              </div>
              {said && (
                <div className="flex min-w-0 items-center gap-1.5 text-[12px] text-text-tertiary">
                  {onHear && seconds !== null && (
                    <button
                      type="button"
                      aria-label={t('meeting.termsHear')}
                      title={t('meeting.termsHear')}
                      onClick={() => onHear(seconds)}
                      className="flex h-5 w-5 shrink-0 cursor-pointer items-center justify-center rounded-full border-none bg-bg-primary text-text-secondary hover:text-accent"
                    >
                      <Play size={10} />
                    </button>
                  )}
                  <span className="min-w-0 truncate">
                    {said.before}
                    <mark className="bg-transparent font-medium text-text-primary">
                      {said.word}
                    </mark>
                    {said.after}
                  </span>
                </div>
              )}
            </li>
          )
        })}
      </ul>
    </form>
  )
}
