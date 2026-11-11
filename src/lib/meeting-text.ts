import type { MeetingSegment } from './tauri'

export interface Paragraph {
  speaker: number | null
  startMs: number
  text: string
  /** The same words in another language, when the meeting is translated as it goes. */
  translation?: string
}

function joined(before: string, after: string): string {
  const latin = (char: string | undefined) => !!char && /[!-~]/.test(char)
  return latin(before[before.length - 1]) && latin(after[0]) ? `${before} ${after}` : before + after
}

/** A speaker who goes on longer than this gets a new paragraph with its own time, so that
 * the text stays readable and each time in it is close to the words under it. */
const LONGEST_PARAGRAPH_MS = 30_000

/** The paragraphs with one more piece of the meeting: the same speaker going on, or a
 * voice too short to tell, continues the last paragraph, unless that one is `closed`
 * because the user is changing it. */
export function addSegment(
  paragraphs: Paragraph[],
  segment: MeetingSegment,
  closed = false,
): Paragraph[] {
  const text = segment.text.trim()
  if (!text) return paragraphs
  const last = paragraphs[paragraphs.length - 1]
  const sameVoice =
    segment.speaker === null
      ? last?.speaker === null
      : segment.speaker === 0 || segment.speaker === last?.speaker
  const goesOn =
    !closed && last && segment.start_ms - last.startMs < LONGEST_PARAGRAPH_MS && sameVoice
  if (goesOn) {
    return [...paragraphs.slice(0, -1), { ...last, text: joined(last.text, text) }]
  }
  return [...paragraphs, { speaker: segment.speaker, startMs: segment.start_ms, text }]
}

/** The paragraphs with a translated piece joined to the paragraph it was said in. */
export function addTranslation(paragraphs: Paragraph[], index: number, text: string): Paragraph[] {
  const paragraph = paragraphs[index]
  const translated = text.trim()
  if (!paragraph || !translated) return paragraphs
  const translation = paragraph.translation ? `${paragraph.translation} ${translated}` : translated
  return paragraphs.map((each, at) => (at === index ? { ...each, translation } : each))
}

/** The paragraphs with the words of one of them as the user corrected them. */
export function withParagraphText(
  paragraphs: Paragraph[],
  index: number,
  text: string,
): Paragraph[] {
  const corrected = text.replace(/\s*\n\s*/g, ' ').trim()
  const paragraph = paragraphs[index]
  if (!paragraph || !corrected || corrected === paragraph.text) return paragraphs
  return paragraphs.map((each, at) => (at === index ? { ...each, text: corrected } : each))
}

/** Which lines of a transcript are words someone said, not a heading or a blank line. */
export function spokenLines(text: string, speakerLabel: string): number[] {
  return transcriptLines(text, speakerLabel).flatMap((line, index) =>
    'text' in line && line.text.trim() !== '' ? [index] : [],
  )
}

/** The transcript with the lines numbered in `at` replaced, one for one. A replacement
 * that is missing or empty leaves its line as it was. */
export function withLines(text: string, at: number[], lines: (string | undefined)[]): string {
  const all = text.split('\n')
  at.forEach((index, order) => {
    const line = lines[order]?.replace(/\s*\n\s*/g, ' ').trim()
    if (line && index < all.length) all[index] = line
  })
  return all.join('\n')
}

const STAMP = /^\[(\d[\d:]*)\] ?/

/** A note being typed during a meeting, with the line the caret is in marked with the
 * time it was begun at, so that the note can later be found in the recording. A line that
 * is empty or already has its time is left alone. */
export function stampNote(
  value: string,
  caret: number,
  seconds: number,
): { value: string; caret: number } {
  const start = value.lastIndexOf('\n', caret - 1) + 1
  const stop = value.indexOf('\n', start)
  const line = value.slice(start, stop < 0 ? undefined : stop)
  if (line.trim() === '' || STAMP.test(line)) return { value, caret }
  const stamp = `[${clock(seconds)}] `
  return { value: value.slice(0, start) + stamp + value.slice(start), caret: caret + stamp.length }
}

/** The notes that carry the time they were written at, with that time in seconds. */
export function stampedNotes(notes: string): { seconds: number; time: string; text: string }[] {
  return notes.split('\n').flatMap((line) => {
    const found = STAMP.exec(line)
    const text = found ? line.slice(found[0].length).trim() : ''
    if (!found || !text) return []
    const seconds = found[1].split(':').reduce((total, part) => total * 60 + Number(part), 0)
    return [{ seconds, time: found[1], text }]
  })
}

export function clock(seconds: number): string {
  const two = (value: number) => String(value).padStart(2, '0')
  const hours = Math.floor(seconds / 3600)
  const rest = `${two(Math.floor(seconds / 60) % 60)}:${two(seconds % 60)}`
  return hours > 0 ? `${hours}:${rest}` : rest
}

/** The line above a paragraph: who speaks and when, or only when where speakers are not
 * told apart. */
export function heading(paragraph: Paragraph, speakerLabel: string): string {
  const time = clock(Math.floor(paragraph.startMs / 1000))
  if (paragraph.speaker === null) return time
  const who = paragraph.speaker === 0 ? speakerLabel : `${speakerLabel} ${paragraph.speaker}`
  return `${who}  ${time}`
}

/** The meeting as text, the way a transcribed file is written out. */
export function meetingText(paragraphs: Paragraph[], speakerLabel: string): string {
  return paragraphs
    .map((paragraph) => {
      const above = heading(paragraph, speakerLabel)
      const said = paragraph.translation
        ? `${paragraph.text}\n${paragraph.translation}`
        : paragraph.text
      return `${above}\n${said}`
    })
    .join('\n\n')
}

function headingPattern(speakerLabel: string): RegExp {
  const label = speakerLabel.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  return new RegExp(`^${label} (\\d+)(?=  \\d)`, 'gm')
}

/** The speaker numbers a text names in its headings, lowest first. */
export function speakersIn(text: string, speakerLabel: string): number[] {
  const found = new Set<number>()
  for (const match of text.matchAll(headingPattern(speakerLabel))) found.add(Number(match[1]))
  return [...found].sort((a, b) => a - b)
}

/** The text with each named speaker's headings carrying the name. */
export function renameSpeakers(
  text: string,
  speakerLabel: string,
  names: Record<number, string>,
): string {
  return text.replace(headingPattern(speakerLabel), (heading, number) => {
    const name = names[Number(number)]?.trim()
    return name || heading
  })
}

export type TranscriptLine = { speaker: number | null; time: string } | { text: string }

/** A transcript line by line: a heading tells who speaks (0 for a voice too short to tell,
 * null where speakers are not told apart) and when, any other line is words. */
export function transcriptLines(text: string, speakerLabel: string): TranscriptLine[] {
  const label = speakerLabel.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const heading = new RegExp(`^${label}(?: (\\d+))? {2}(\\d[\\d:]*)$`)
  return text.split('\n').map((line) => {
    const found = heading.exec(line)
    if (found) return { speaker: Number(found[1] ?? 0), time: found[2] }
    return /^(\d+:)?\d{2}:\d{2}$/.test(line) ? { speaker: null, time: line } : { text: line }
  })
}

/** A stretch of a recording one person speaks in, as the speaker models tell it. */
export interface SpeakerTurn {
  start_ms: number
  end_ms: number
  speaker: number
}

/** Who speaks each paragraph, told again from the whole recording, which tells voices
 * apart better than the windows it was first heard in. A voice keeps the number it mostly
 * had, so that names given to numbers stay with their people; a voice that is new gets a
 * new number. A paragraph nobody is heard in keeps what it had. */
export function speakersAgain(paragraphs: Paragraph[], turns: SpeakerTurn[]): (number | null)[] {
  const heard = paragraphs.map((paragraph, index) => {
    if (paragraph.speaker === null) return null
    const until = paragraphs[index + 1]?.startMs ?? Infinity
    let most: { voice: number; ms: number } | null = null
    const each = new Map<number, number>()
    for (const turn of turns) {
      const ms = Math.min(until, turn.end_ms) - Math.max(paragraph.startMs, turn.start_ms)
      if (ms <= 0) continue
      const total = (each.get(turn.speaker) ?? 0) + ms
      each.set(turn.speaker, total)
      if (!most || total > most.ms) most = { voice: turn.speaker, ms: total }
    }
    return most as { voice: number; ms: number } | null
  })
  // How long each voice spoke under each of the numbers given while recording.
  const had = new Map<number, Map<number, number>>()
  heard.forEach((found, index) => {
    if (!found) return
    const numbers = had.get(found.voice) ?? new Map<number, number>()
    const before = paragraphs[index].speaker ?? 0
    numbers.set(before, (numbers.get(before) ?? 0) + found.ms)
    had.set(found.voice, numbers)
  })
  const spoke = (voice: number) => [...(had.get(voice)?.values() ?? [])].reduce((a, b) => a + b, 0)
  const taken = new Set<number>()
  let highest = Math.max(0, ...paragraphs.map((paragraph) => paragraph.speaker ?? 0))
  const number = new Map<number, number>()
  for (const voice of [...had.keys()].sort((a, b) => spoke(b) - spoke(a))) {
    const mostly = [...(had.get(voice) ?? [])]
      .filter(([before]) => before > 0 && !taken.has(before))
      .sort((a, b) => b[1] - a[1])[0]
    const given = mostly ? mostly[0] : ++highest
    taken.add(given)
    number.set(voice, given)
  }
  return paragraphs.map((paragraph, index) => {
    const found = heard[index]
    return found ? (number.get(found.voice) ?? paragraph.speaker) : paragraph.speaker
  })
}

/** The text with its headings naming `speakers`, one for each heading in order. A text
 * whose headings no longer answer them one for one is left as it is. */
export function withSpeakers(
  text: string,
  speakerLabel: string,
  speakers: (number | null)[],
): string {
  const lines = text.split('\n')
  const read = transcriptLines(text, speakerLabel)
  const headings = read.flatMap((line, index) => ('text' in line ? [] : [index]))
  if (headings.length !== speakers.length) return text
  headings.forEach((index, order) => {
    const line = read[index]
    const who = speakers[order]
    if ('text' in line || line.speaker === null || who === null) return
    lines[index] = `${who > 0 ? `${speakerLabel} ${who}` : speakerLabel}  ${line.time}`
  })
  return lines.join('\n')
}

/** The colour a speaker's name is set in. A voice too short to tell has none of its own. */
export function speakerColor(speaker: number): string {
  return speaker > 0 ? `var(--speaker-${((speaker - 1) % 6) + 1})` : 'var(--color-text-secondary)'
}
