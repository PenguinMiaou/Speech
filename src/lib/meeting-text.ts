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
  const goesOn =
    !closed &&
    last &&
    segment.speaker !== null &&
    segment.start_ms - last.startMs < LONGEST_PARAGRAPH_MS &&
    (segment.speaker === 0 || segment.speaker === last.speaker)
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

/** The line above a paragraph: who speaks and when. Null when speakers are not told apart. */
export function heading(paragraph: Paragraph, speakerLabel: string): string | null {
  if (paragraph.speaker === null) return null
  const who = paragraph.speaker === 0 ? speakerLabel : `${speakerLabel} ${paragraph.speaker}`
  return `${who}  ${clock(Math.floor(paragraph.startMs / 1000))}`
}

/** The meeting as text, the way a transcribed file is written out. */
export function meetingText(paragraphs: Paragraph[], speakerLabel: string): string {
  return paragraphs
    .map((paragraph) => {
      const above = heading(paragraph, speakerLabel)
      const said = paragraph.translation
        ? `${paragraph.text}\n${paragraph.translation}`
        : paragraph.text
      return above ? `${above}\n${said}` : said
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

export type TranscriptLine = { speaker: number; time: string } | { text: string }

/** A transcript line by line: a heading tells who speaks (0 for a voice too short to tell)
 * and when, any other line is words. */
export function transcriptLines(text: string, speakerLabel: string): TranscriptLine[] {
  const label = speakerLabel.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const heading = new RegExp(`^${label}(?: (\\d+))? {2}(\\d[\\d:]*)$`)
  return text.split('\n').map((line) => {
    const found = heading.exec(line)
    return found ? { speaker: Number(found[1] ?? 0), time: found[2] } : { text: line }
  })
}

/** The colour a speaker's name is set in. A voice too short to tell has none of its own. */
export function speakerColor(speaker: number): string {
  return speaker > 0 ? `var(--speaker-${((speaker - 1) % 6) + 1})` : 'var(--color-text-secondary)'
}
