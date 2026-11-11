import { Fragment } from 'react'
import { speakerColor, transcriptLines } from '../../lib/meeting-text'

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
}

function seconds(time: string): number {
  return time.split(':').reduce((total, part) => total * 60 + Number(part), 0)
}

/** A meeting's text with each speaker's name in that speaker's colour, so that who speaks
 * stands apart from what is said. Selecting and copying gives the plain text. */
export function Transcript({ text, names, speakerLabel, label, onSeek, onEdit, position }: Props) {
  const lines = transcriptLines(text, speakerLabel)
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
        onClick={() => {
          // Dragging over words selects them; only a plain click moves the sound.
          if (start !== null && window.getSelection()?.isCollapsed !== false) onSeek?.(start, false)
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
              <span className="font-medium" style={{ color: speakerColor(line.speaker) }}>
                {names[line.speaker]?.trim() ||
                  (line.speaker > 0 ? `${speakerLabel} ${line.speaker}` : speakerLabel)}
              </span>
              {'  '}
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
