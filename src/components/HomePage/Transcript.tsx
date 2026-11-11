import { Fragment, useEffect, useRef, useState } from 'react'
import { changes } from '../../lib/word-diff'
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
  /** The text as it was before it was put right. Where a line differs from it, what was
   * taken out is struck through and what was put in is marked. */
  before?: string
  /** Given when who speaks can be put right: called with the heading's line, the speaker
   * it now belongs to, and whether all that the speaker before said goes along. */
  onSpeaker?: (line: number, speaker: number, all: boolean) => void
  /** What the choice to take all a speaker said along is called. */
  allLabel?: string
  /** Given when a speaker can be named where they speak, with what that field is called. */
  onName?: (speaker: number, name: string) => void
  nameLabel?: string
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
  before,
  onSpeaker,
  allLabel,
  onName,
  nameLabel,
}: Props) {
  const lines = transcriptLines(text, speakerLabel)
  const was = before === undefined ? [] : transcriptLines(before, speakerLabel)
  const earlier = (index: number) => {
    const line = was.length === lines.length ? was[index] : undefined
    return line && 'text' in line ? line.text : undefined
  }
  const called = (speaker: number) =>
    names[speaker]?.trim() || (speaker > 0 ? `${speakerLabel} ${speaker}` : speakerLabel)
  // The heading whose speaker is being put right, and whether all they said goes along.
  const [choosing, setChoosing] = useState<number | null>(null)
  const [all, setAll] = useState(false)
  const numbers = [
    ...new Set(lines.flatMap((line) => ('text' in line || !line.speaker ? [] : [line.speaker]))),
  ].sort((a, b) => a - b)
  // Who spoke just before each heading, by the name shown.
  let last: string | null = null
  const goesOn = lines.map((line) => {
    if ('text' in line || line.speaker === null) return false
    const name = called(line.speaker)
    const same = name === last
    last = name
    return same
  })
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
    const old = earlier(index)
    if (old !== undefined && old !== line.text) {
      return (
        <span data-line={index}>
          {changes(old, line.text).map((part, at) =>
            part.kind === 'gone' ? (
              <del key={at} className="select-none text-text-tertiary">
                {part.text}
              </del>
            ) : part.kind === 'new' ? (
              <ins key={at} className="rounded-[3px] bg-accent/10 text-accent no-underline">
                {part.text}
              </ins>
            ) : (
              part.text
            ),
          )}
        </span>
      )
    }
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
              {line.speaker !== null && !goesOn[index] && (
                <>
                  {onSpeaker ? (
                    <span className="relative">
                      <button
                        type="button"
                        aria-expanded={choosing === index}
                        onDoubleClick={() => {
                          setAll(false)
                          setChoosing(choosing === index ? null : index)
                        }}
                        className="cursor-pointer border-none bg-transparent p-0 font-medium"
                        style={{ color: speakerColor(line.speaker) }}
                      >
                        {called(line.speaker)}
                      </button>
                      {choosing === index && (
                        <span
                          role="menu"
                          className="absolute left-0 top-full z-20 mt-1 flex min-w-[168px] select-none flex-col whitespace-nowrap rounded-[8px] border border-border bg-bg-elevated py-1 text-[13px] shadow-lg"
                        >
                          {onName && line.speaker > 0 && (
                            <input
                              autoFocus
                              aria-label={nameLabel}
                              placeholder={nameLabel}
                              defaultValue={names[line.speaker] ?? ''}
                              onKeyDown={(event) => {
                                if (event.key === 'Escape') setChoosing(null)
                                if (event.key !== 'Enter' || line.speaker === null) return
                                onName(line.speaker, event.currentTarget.value.trim())
                                setChoosing(null)
                              }}
                              className="mx-2 mb-1 rounded-[6px] border border-border bg-bg-primary px-2 py-1 text-[13px] text-text-primary outline-none focus:border-accent"
                            />
                          )}
                          {[...numbers, (numbers[numbers.length - 1] ?? 0) + 1]
                            .filter((number) => number !== line.speaker)
                            .map((number) => (
                              <button
                                key={number}
                                type="button"
                                role="menuitem"
                                onClick={() => {
                                  setChoosing(null)
                                  onSpeaker(index, number, all)
                                }}
                                className="cursor-pointer border-none bg-transparent px-3 py-1.5 text-left font-medium hover:bg-bg-secondary"
                                style={{ color: speakerColor(number) }}
                              >
                                {called(number)}
                              </button>
                            ))}
                          {allLabel && (
                            <button
                              type="button"
                              role="menuitemcheckbox"
                              aria-checked={all}
                              onClick={() => setAll(!all)}
                              className="flex cursor-pointer items-center gap-2 border-x-0 border-b-0 border-t border-border bg-transparent px-3 py-1.5 text-left text-text-secondary hover:bg-bg-secondary"
                            >
                              <span
                                className={`h-3 w-3 rounded-[3px] border border-border ${all ? 'bg-accent' : ''}`}
                              />
                              {allLabel}
                            </button>
                          )}
                        </span>
                      )}
                    </span>
                  ) : (
                    <span className="font-medium" style={{ color: speakerColor(line.speaker) }}>
                      {called(line.speaker)}
                    </span>
                  )}
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
