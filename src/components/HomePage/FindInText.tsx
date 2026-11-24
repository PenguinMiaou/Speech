import { forwardRef, useEffect, useImperativeHandle, useRef, useState, type RefObject } from 'react'
import { useTranslation } from 'react-i18next'
import { ChevronDown, ChevronUp, Search, X } from 'lucide-react'

export interface FindHandle {
  /** Puts the keyboard in the search field. */
  focus: () => void
}

interface Props {
  /** What is searched: the text shown inside it. */
  within: RefObject<HTMLElement | null>
  /** Changes whenever the text does, so that the places found are read again. */
  version: string
  /** The speakers of the text, by number, with the name each is shown under. */
  speakers: { number: number; name: string }[]
}

/** The places in `root` where `query` stands, as ranges of its text, case not minded. */
export function findIn(root: HTMLElement, query: string): Range[] {
  const wanted = query.trim().toLocaleLowerCase()
  if (!wanted) return []
  const found: Range[] = []
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const text = (node.nodeValue ?? '').toLocaleLowerCase()
    for (let at = text.indexOf(wanted); at >= 0; at = text.indexOf(wanted, at + wanted.length)) {
      const range = document.createRange()
      range.setStart(node, at)
      range.setEnd(node, at + wanted.length)
      found.push(range)
    }
  }
  return found
}

/** The browser's way of marking text without changing it, where there is one. */
type Highlights = { set: (name: string, value: unknown) => void; delete: (name: string) => void }
const highlights = (): Highlights | null =>
  (globalThis.CSS as unknown as { highlights?: Highlights } | undefined)?.highlights ?? null
const Highlight = (globalThis as unknown as { Highlight?: new (...ranges: Range[]) => unknown })
  .Highlight

function show(range: Range) {
  const element = range.startContainer.parentElement
  element?.scrollIntoView?.({ block: 'center', behavior: 'smooth' })
}

/** Finds words in a meeting's text, marking every place they stand and moving from one to
 * the next; or goes from one section of a speaker to the next. */
export const FindInText = forwardRef<FindHandle, Props>(function FindInText(
  { within, version, speakers },
  ref,
) {
  const { t } = useTranslation()
  const field = useRef<HTMLInputElement>(null)
  const [query, setQuery] = useState('')
  const [places, setPlaces] = useState<Range[]>([])
  const [current, setCurrent] = useState(0)
  const [speaker, setSpeaker] = useState<number | null>(null)
  const turn = useRef(-1)
  useImperativeHandle(ref, () => ({ focus: () => field.current?.select() }))

  useEffect(() => {
    const root = within.current
    const found = root ? findIn(root, query) : []
    setPlaces(found)
    setCurrent(0)
    const marks = highlights()
    if (!marks || !Highlight) return
    if (found.length > 0) marks.set('find', new Highlight(...found))
    else marks.delete('find')
  }, [query, version, within])

  useEffect(() => {
    const marks = highlights()
    const place = places[current]
    if (place) show(place)
    if (!marks || !Highlight) return
    if (place) marks.set('find-current', new Highlight(place))
    else marks.delete('find-current')
  }, [places, current])

  useEffect(
    () => () => {
      highlights()?.delete('find')
      highlights()?.delete('find-current')
    },
    [],
  )

  const step = (by: number) =>
    places.length > 0 && setCurrent((at) => (at + by + places.length) % places.length)
  /** Goes to the next section the chosen speaker begins. */
  const nextTurn = (number: number, by = 1) => {
    const sections = [
      ...(within.current?.querySelectorAll<HTMLElement>(`[data-speaker="${number}"]`) ?? []),
    ]
    if (sections.length === 0) return
    turn.current = (turn.current + by + sections.length) % sections.length
    sections[turn.current].scrollIntoView?.({ block: 'start', behavior: 'smooth' })
  }

  return (
    <div className="flex flex-wrap items-center gap-2 text-[12px] text-text-tertiary">
      <label className="flex min-w-[180px] flex-1 items-center gap-1.5 rounded-md border border-border px-2">
        <Search size={13} aria-hidden />
        <input
          ref={field}
          type="search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault()
              step(event.shiftKey ? -1 : 1)
            }
            if (event.key === 'Escape') setQuery('')
          }}
          aria-label={t('meeting.findInText')}
          placeholder={t('meeting.findInText')}
          className="h-7 min-w-0 flex-1 border-none bg-transparent text-[13px] text-text-primary outline-none placeholder:text-text-tertiary"
        />
        {query && (
          <>
            <span aria-live="polite" className="tabular-nums">
              {places.length > 0
                ? t('meeting.foundAt', { at: current + 1, count: places.length })
                : t('meeting.foundNone')}
            </span>
            <button
              type="button"
              aria-label={t('meeting.findPrevious')}
              onClick={() => step(-1)}
              className="btn-quiet px-1"
            >
              <ChevronUp size={13} />
            </button>
            <button
              type="button"
              aria-label={t('meeting.findNext')}
              onClick={() => step(1)}
              className="btn-quiet px-1"
            >
              <ChevronDown size={13} />
            </button>
            <button
              type="button"
              aria-label={t('common.close')}
              onClick={() => setQuery('')}
              className="btn-quiet px-1"
            >
              <X size={13} />
            </button>
          </>
        )}
      </label>
      {speakers.length > 1 && (
        <span className="flex items-center gap-1">
          <select
            aria-label={t('meeting.jumpToSpeaker')}
            value={speaker ?? ''}
            onChange={(event) => {
              const number = event.target.value === '' ? null : Number(event.target.value)
              setSpeaker(number)
              turn.current = -1
              if (number !== null) nextTurn(number)
            }}
            className="h-7 cursor-pointer rounded-md border border-border bg-transparent px-1.5 text-[12px] text-text-secondary outline-none"
          >
            <option value="">{t('meeting.jumpToSpeaker')}</option>
            {speakers.map((each) => (
              <option key={each.number} value={each.number}>
                {each.name}
              </option>
            ))}
          </select>
          {speaker !== null && (
            <>
              <button
                type="button"
                aria-label={t('meeting.previousTurn')}
                onClick={() => nextTurn(speaker, -1)}
                className="btn-quiet px-1"
              >
                <ChevronUp size={13} />
              </button>
              <button
                type="button"
                aria-label={t('meeting.nextTurn')}
                onClick={() => nextTurn(speaker)}
                className="btn-quiet px-1"
              >
                <ChevronDown size={13} />
              </button>
            </>
          )}
        </span>
      )}
    </div>
  )
})
