import { Fragment } from 'react'
import { speakerColor, transcriptLines } from '../../lib/meeting-text'

interface Props {
  text: string
  names: Record<number, string>
  speakerLabel: string
  label: string
  /** Given when the meeting's sound can be played: a time then jumps to that moment. */
  onSeek?: (seconds: number) => void
}

function seconds(time: string): number {
  return time.split(':').reduce((total, part) => total * 60 + Number(part), 0)
}

/** A meeting's text with each speaker's name in that speaker's colour, so that who speaks
 * stands apart from what is said. Selecting and copying gives the plain text. */
export function Transcript({ text, names, speakerLabel, label, onSeek }: Props) {
  return (
    <div
      role="textbox"
      aria-readonly="true"
      aria-multiline="true"
      aria-label={label}
      tabIndex={0}
      className="select-text whitespace-pre-wrap text-[14px] leading-relaxed text-text-primary outline-none"
    >
      {transcriptLines(text, speakerLabel).map((line, index) => (
        <Fragment key={index}>
          {index > 0 && '\n'}
          {'text' in line ? (
            line.text
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
                  onClick={() => onSeek(seconds(line.time))}
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
