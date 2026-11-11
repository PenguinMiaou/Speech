import { changes } from './word-diff'
import { plainOf } from './meeting-format'
import { clock, secondsAt, transcriptLines, type Times } from './meeting-text'

/** The mark a model puts in a long paragraph where the subject changes. */
export const BREAK = '¶'

const WEEKDAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday']
const DAYS_BEFORE = 14
const DAYS_AFTER = 35

/** The day a meeting was held and the days around it, each with its weekday: what a
 * model needs to tell which date "next Wednesday" was, without reckoning it. */
export function calendarOf(startedAt: number): string {
  const day = (offset: number) => {
    const at = new Date(startedAt)
    at.setDate(at.getDate() + offset)
    const two = (value: number) => String(value).padStart(2, '0')
    return `${at.getFullYear()}-${two(at.getMonth() + 1)}-${two(at.getDate())} ${WEEKDAYS[at.getDay()]}`
  }
  const days = Array.from({ length: DAYS_BEFORE + 1 + DAYS_AFTER }, (_, index) =>
    day(index - DAYS_BEFORE),
  )
  return [`The meeting was held on ${day(0)}.`, ...days].join('\n')
}

/** How many characters a second a paragraph is taken to be said at, where nothing else
 * tells how long it took. */
const CHARS_A_SECOND = 4

function secondsOf(time: string): number {
  return time.split(':').reduce((total, part) => total * 60 + Number(part), 0)
}

/** What was heard, cut where the words put right of it were cut: `parts` are those
 * words in pieces, and each piece of the answer is what its part was made of. */
function cutLike(old: string, parts: string[]): string[] {
  const starts: number[] = []
  let at = 0
  for (const part of parts.slice(0, -1)) {
    at += part.length + 1
    starts.push(at)
  }
  const out: string[] = []
  let current = ''
  let read = 0
  for (const change of changes(old, parts.join(' '))) {
    if (change.kind === 'gone') {
      current += change.text
      continue
    }
    let rest = change.text
    while (out.length < starts.length && read + rest.length > starts[out.length]) {
      const take = starts[out.length] - read
      if (change.kind === 'same') current += rest.slice(0, take)
      out.push(current.trim())
      current = ''
      rest = rest.slice(take)
      read += take
    }
    if (change.kind === 'same') current += rest
    read += rest.length
  }
  out.push(current.trim())
  while (out.length < parts.length) out.push('')
  return out
}

/** The times of one paragraph, cut at the places `at` of a text `length` long. */
function cutTimes(times: Times, length: number, at: number[], begins: number[]): Times[] {
  const fit = times.chars / length
  const cuts = [0, ...at.map((place) => Math.round(place * fit)), times.chars]
  return cuts.slice(0, -1).map((from, index) => {
    const to = cuts[index + 1]
    const began = index === 0 ? (times.marks[0]?.[1] ?? 0) : begins[index - 1] * 1000
    const marks = times.marks
      .filter(([place]) => place >= from && place < to)
      .map(([place, ms]): [number, number] => [place - from, ms])
    if (marks[0]?.[0] !== 0) marks.unshift([0, began])
    return {
      end_ms:
        index + 1 < begins.length + 1 && index < begins.length
          ? begins[index] * 1000
          : times.end_ms,
      chars: to - from,
      marks,
    }
  })
}

/** The transcript with each paragraph that carries the mark of a new subject begun anew
 * there: the same speaker, at the time those words were said. `times` are cut along
 * with the words while they answer the headings one for one. With `before`, the text as
 * it was heard, line for line, that comes back cut at the same places as `shaped`, so
 * that each new paragraph can be compared with what it was made of. */
export function withParagraphsSplit(
  text: string,
  speakerLabel: string,
  times: Times[],
  before?: string,
): { text: string; times: Times[]; shaped: string | undefined } {
  if (!text.includes(BREAK)) return { text, times, shaped: undefined }
  const parsed = transcriptLines(text, speakerLabel)
  const lines = text.split('\n')
  const heard = before?.split('\n')
  const shape = heard?.length === lines.length
  const headings = parsed.flatMap((line, index) => ('text' in line ? [] : [index]))
  const timed = times.length === headings.length
  const out: string[] = []
  const old: string[] = []
  const cut: Times[] = []
  let order = -1
  parsed.forEach((line, index) => {
    if (!('text' in line)) {
      order += 1
      out.push(lines[index])
      old.push(heard?.[index] ?? lines[index])
      return
    }
    const above = index > 0 ? parsed[index - 1] : undefined
    const parts = line.text
      .split(BREAK)
      .map((part) => part.trim())
      .filter(Boolean)
    const own = above && !('text' in above)
    if (!own || parts.length < 2) {
      out.push(line.text.includes(BREAK) ? parts.join(' ') : lines[index])
      old.push(heard?.[index] ?? '')
      if (own && timed) cut.push(times[order])
      return
    }
    const length = parts.reduce((sum, part) => sum + plainOf(part).length, 0)
    const start = secondsOf(above.time)
    const next = headings[order + 1]
    const nextLine = next === undefined ? undefined : parsed[next]
    const end =
      nextLine && !('text' in nextLine) ? secondsOf(nextLine.time) : start + length / CHARS_A_SECOND
    const places: number[] = []
    const begins: number[] = []
    let place = 0
    parts.slice(0, -1).forEach((part) => {
      place += plainOf(part).length
      const known = timed ? secondsAt(times[order], length, place) : null
      const guessed = start + ((end - start) * place) / length
      const last = begins[begins.length - 1] ?? start
      places.push(place)
      begins.push(Math.max(last, Math.round(known ?? guessed)))
    })
    const was = shape ? cutLike(heard[index], parts) : []
    parts.forEach((part, at) => {
      if (at > 0) {
        const time = clock(begins[at - 1])
        const who =
          above.speaker === null
            ? time
            : `${speakerLabel}${above.speaker > 0 ? ` ${above.speaker}` : ''}  ${time}`
        out.push('', who)
        old.push('', who)
      }
      out.push(part)
      old.push(was[at] ?? '')
    })
    if (timed) cut.push(...cutTimes(times[order], length, places, begins))
  })
  return {
    text: out.join('\n'),
    times: timed ? cut : times,
    shaped: shape ? old.join('\n') : undefined,
  }
}
