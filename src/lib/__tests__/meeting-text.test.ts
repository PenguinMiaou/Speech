import { describe, expect, it } from 'vitest'
import { addSegment, meetingText, type Paragraph } from '../meeting-text'

const said = (speaker: number | null, start_ms: number, text: string) => ({
  speaker,
  start_ms,
  text,
})

describe('meeting text', () => {
  it('starts a paragraph when another speaker takes over', () => {
    let paragraphs: Paragraph[] = []
    paragraphs = addSegment(paragraphs, said(1, 0, '你好'))
    paragraphs = addSegment(paragraphs, said(2, 4_000, '收到'))
    expect(paragraphs).toEqual([
      { speaker: 1, startMs: 0, text: '你好' },
      { speaker: 2, startMs: 4_000, text: '收到' },
    ])
  })

  it('lets a speaker go on across two windows in one paragraph', () => {
    let paragraphs = addSegment([], said(1, 0, 'the first half'))
    paragraphs = addSegment(paragraphs, said(1, 30_000, 'and the rest'))
    expect(paragraphs).toEqual([{ speaker: 1, startMs: 0, text: 'the first half and the rest' }])
  })

  it('gives a voice too short to tell to whoever spoke last', () => {
    let paragraphs = addSegment([], said(2, 0, '我觉得可以'))
    paragraphs = addSegment(paragraphs, said(0, 5_000, '对'))
    expect(paragraphs).toEqual([{ speaker: 2, startMs: 0, text: '我觉得可以对' }])
  })

  it('keeps each window its own paragraph when speakers are not told apart', () => {
    let paragraphs = addSegment([], said(null, 0, 'one'))
    paragraphs = addSegment(paragraphs, said(null, 30_000, 'two'))
    paragraphs = addSegment(paragraphs, said(null, 60_000, '  '))
    expect(meetingText(paragraphs, 'Speaker')).toBe('one\n\ntwo')
  })

  it('writes the meeting out with who spoke and when', () => {
    const paragraphs = [
      { speaker: 1, startMs: 0, text: '你好' },
      { speaker: 2, startMs: 3_725_000, text: '收到' },
      { speaker: 0, startMs: 4_000_000, text: '嗯' },
    ]
    expect(meetingText(paragraphs, '说话人')).toBe(
      '说话人 1  00:00\n你好\n\n说话人 2  1:02:05\n收到\n\n说话人  1:06:40\n嗯',
    )
  })
})
