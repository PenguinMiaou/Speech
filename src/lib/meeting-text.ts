import type { MeetingSegment } from './tauri'

export interface Paragraph {
  speaker: number | null
  startMs: number
  text: string
}

function joined(before: string, after: string): string {
  const latin = (char: string | undefined) => !!char && /[!-~]/.test(char)
  return latin(before[before.length - 1]) && latin(after[0]) ? `${before} ${after}` : before + after
}

/** The paragraphs with one more piece of the meeting: the same speaker going on, or a
 * voice too short to tell, continues the last paragraph. */
export function addSegment(paragraphs: Paragraph[], segment: MeetingSegment): Paragraph[] {
  const text = segment.text.trim()
  if (!text) return paragraphs
  const last = paragraphs[paragraphs.length - 1]
  const goesOn =
    last && segment.speaker !== null && (segment.speaker === 0 || segment.speaker === last.speaker)
  if (goesOn) {
    return [...paragraphs.slice(0, -1), { ...last, text: joined(last.text, text) }]
  }
  return [...paragraphs, { speaker: segment.speaker, startMs: segment.start_ms, text }]
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
      return above ? `${above}\n${paragraph.text}` : paragraph.text
    })
    .join('\n\n')
}
