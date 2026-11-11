import { Fragment, useEffect, useRef } from 'react'
import {
  charsSaid,
  secondsAt,
  speakerColor,
  transcriptLines,
  type Times,
} from '../../lib/meeting-text'

interface Props {
  text: string
  names: Record<number, string>
  speakerLabel: string
  label: string
  /** Given when the meeting's sound can be played: a time, or the words said then, jump to
   * that moment. Only a time also starts playing. */
  onSeek?: (seconds: number, play: boolean) => void
  /** Given when the words can be changed: called with the line and what it now says. */
  onEdit?: (line: number, before: string, after: string) => void
  /** Where the sound is, in seconds. Words said later than that are greyed. */
  position?: number
  /** When the words under each heading were said, in order. With these a click goes to
   * the word, and the paragraph being played shows how far the sound has got. */
  times?: Times[]
}

/** The place in the words that was clicked, where the browser can tell. */
function clickedAt(event: { clientX: number; clientY: number }, words: Node): number | null {
  const at = document.caretRangeFromPoint?.(event.clientX, event.clientY)
  return at && words.contains(at.startContainer) ? at.startOffset : null
}

function seconds(time: string): number {
  return time.split(':').reduce((total, part) => total * 60 + Number(part), 0)
}

/** A meeting's text with each speaker's name in that speaker's colour, so that who speaks
 * stands apart from what is said. Selecting and copying gives the plain text. */
export function Transcript({
  text,
  names,
  speakerLabel,
  label,
  onSeek,
  onEdit,
  position,
  times,
}: Props) {
  const lines = transcriptLines(text, speakerLabel)
  const headings = lines.filter((line) => !('text' in line)).length
  // The times belong to the words right under each heading, while they answer the
  // headings one for one.
  let order = -1
  const timed = lines.map((line, index) => {
    if (!('text' in line)) order += 1
    const under = index > 0 && !('text' in lines[index - 1])
    return under && times?.length === headings ? times[order] : undefined
  })
  const root = useRef<HTMLDivElement>(null)
  // The paragraph being played, and how many of its characters have been said.
  let playing: { line: number; said: number } | null = null
  if (position !== undefined && position > 0) {
    lines.forEach((line, index) => {
      if (!('text' in line) || !timed[index]) return
      const said = charsSaid(timed[index], line.text.length, position)
      const begun = (timed[index]?.marks[0]?.[1] ?? Infinity) <= position * 1000
      if (said !== null && begun) playing = { line: index, said }
    })
  }
  const { line: playingLine, said: playingSaid } = playing ?? { line: -1, said: 0 }
  useEffect(() => {
    // The words still to come are marked without touching the text, which may be in
    // the middle of being changed.
    const marks = globalThis.CSS?.highlights
    if (!marks || typeof Highlight === 'undefined') return
    const words = root.current?.querySelector(`[data-line="${playingLine}"]`)?.firstChild
    const length = words?.textContent?.length ?? 0
    if (!words || words.nodeType !== Node.TEXT_NODE || playingSaid >= length) {
      marks.delete('ahead')
      return
    }
    const ahead = document.createRange()
    ahead.setStart(words, playingSaid)
    ahead.setEnd(words, length)
    marks.set('ahead', new Highlight(ahead))
    return () => void marks.delete('ahead')
  }, [playingLine, playingSaid, text])
  // When each line was said: the time in the heading above it.
  let said: number | null = null
  const starts = lines.map((line) => {
    if (!('text' in line)) said = seconds(line.time)
    return said
  })
  const words = (line: { text: string }, index: number) => {
    const start = starts[index]
    if (line.text.trim() === '' || (!onEdit && start === null)) return line.text
    const ahead = start !== null && position !== undefined && position > 0 && start > position
    return (
      <span
        contentEditable={onEdit ? 'plaintext-only' : undefined}
        suppressContentEditableWarning
        spellCheck={false}
        data-line={index}
        onClick={(event) => {
          // Dragging over words selects them; only a plain click moves the sound.
          if (start === null || window.getSelection()?.isCollapsed === false) return
          const at = clickedAt(event, event.currentTarget)
          const exact = at === null ? null : secondsAt(timed[index], line.text.length, at)
          onSeek?.(exact ?? start, false)
        }}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.preventDefault()
            event.currentTarget.blur()
          }
        }}
        onBlur={(event) => {
          if (!onEdit) return
          const after = (event.currentTarget.textContent ?? '').trim()
          if (after && after !== line.text) onEdit(index, line.text, after)
          else event.currentTarget.textContent = line.text
        }}
        className={`rounded-[3px] outline-none transition-colors focus:bg-bg-secondary ${
          ahead ? 'text-text-tertiary' : ''
        }`}
      >
        {line.text}
      </span>
    )
  }
  return (
    <div
      ref={root}
      role={onEdit ? 'group' : 'textbox'}
      aria-readonly={onEdit ? undefined : 'true'}
      aria-multiline="true"
      aria-label={label}
      tabIndex={onEdit ? undefined : 0}
      className="select-text whitespace-pre-wrap text-[14px] leading-relaxed text-text-primary outline-none"
    >
      {lines.map((line, index) => (
        <Fragment key={index}>
          {index > 0 && '\n'}
          {'text' in line ? (
            words(line, index)
          ) : (
            <>
              {line.speaker !== null && (
                <>
                  <span className="font-medium" style={{ color: speakerColor(line.speaker) }}>
                    {names[line.speaker]?.trim() ||
                      (line.speaker > 0 ? `${speakerLabel} ${line.speaker}` : speakerLabel)}
                  </span>
                  {'  '}
                </>
              )}
              {onSeek ? (
                <button
                  type="button"
                  onClick={() => onSeek(seconds(line.time), true)}
                  className="cursor-pointer border-none bg-transparent p-0 text-[12px] tabular-nums text-text-tertiary hover:text-accent"
                >
                  {line.time}
                </button>
              ) : (
                <span className="text-[12px] tabular-nums text-text-tertiary">{line.time}</span>
              )}
            </>
          )}
        </Fragment>
      ))}
    </div>
  )
}
