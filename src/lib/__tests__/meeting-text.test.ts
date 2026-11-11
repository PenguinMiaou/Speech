import { describe, expect, it } from 'vitest'
import {
  addSegment,
  addTranslation,
  meetingText,
  renameSpeakers,
  speakersIn,
  spokenLines,
  stampNote,
  stampedNotes,
  transcriptLines,
  withLines,
  withParagraphText,
  type Paragraph,
} from '../meeting-text'

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
    paragraphs = addSegment(paragraphs, said(1, 12_000, 'and the rest'))
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

  it('finds the speakers a text names', () => {
    const text = '说话人 2  00:00\n你好\n\n说话人 1  00:05\n说话人 9 是谁\n\n说话人 2  00:09\n好'
    expect(speakersIn(text, '说话人')).toEqual([1, 2])
    expect(speakersIn('plain text', 'Speaker')).toEqual([])
  })

  it('puts a name in place of a speaker number, in headings only', () => {
    const text =
      'Speaker 1  00:00\nSpeaker 2 said so\n\nSpeaker 2  1:02:05\nyes\n\nSpeaker 12  1:03:00\nno'
    expect(renameSpeakers(text, 'Speaker', { 1: 'Ann', 2: '  ', 12: 'Bo (PM)' })).toBe(
      'Ann  00:00\nSpeaker 2 said so\n\nSpeaker 2  1:02:05\nyes\n\nBo (PM)  1:03:00\nno',
    )
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

  it('puts a translation under the paragraph it belongs to and joins later ones to it', () => {
    let paragraphs = addSegment([], said(1, 0, '周五上线'))
    paragraphs = addSegment(paragraphs, said(2, 5000, '好的'))
    paragraphs = addTranslation(paragraphs, 0, 'We ship on Friday.')
    paragraphs = addTranslation(paragraphs, 0, 'Testing first.')
    expect(paragraphs[0].translation).toBe('We ship on Friday. Testing first.')
    expect(paragraphs[1].translation).toBeUndefined()
    expect(addTranslation(paragraphs, 7, 'lost')).toBe(paragraphs)
  })

  it('writes a translation out under its paragraph', () => {
    const paragraphs = addTranslation(
      addSegment([], said(1, 0, '周五上线')),
      0,
      'We ship on Friday.',
    )
    expect(meetingText(paragraphs, '说话人')).toBe('说话人 1  00:00\n周五上线\nWe ship on Friday.')
  })

  it('reads a transcript line by line, telling who speaks from what was said', () => {
    expect(transcriptLines('说话人 2  1:02:05\n收到\n\n说话人  00:07\n嗯', '说话人')).toEqual([
      { speaker: 2, time: '1:02:05' },
      { text: '收到' },
      { text: '' },
      { speaker: 0, time: '00:07' },
      { text: '嗯' },
    ])
  })

  it('gives a speaker who goes on for long a new paragraph with its own time', () => {
    const first = addSegment([], said(1, 0, '开头'))
    const soon = addSegment(first, said(1, 20_000, '接着'))
    expect(soon).toHaveLength(1)
    const later = addSegment(soon, said(1, 31_000, '半分钟后'))
    expect(later.map((each) => each.startMs)).toEqual([0, 31_000])
  })

  it('starts a new paragraph after one the user is changing', () => {
    const first = addSegment([], said(1, 0, '第一句'))
    expect(addSegment(first, said(1, 5_000, '第二句'), true)).toHaveLength(2)
    expect(addSegment(first, said(1, 5_000, '第二句'))).toHaveLength(1)
  })

  it('takes the words of a paragraph as the user corrected them', () => {
    const first = addSegment([], said(1, 0, '我们用套瑞'))
    expect(withParagraphText(first, 0, ' 我们用\nTauri ')[0].text).toBe('我们用 Tauri')
    expect(withParagraphText(first, 0, '  ')).toBe(first)
    expect(withParagraphText(first, 3, '别的')).toBe(first)
  })

  it('replaces only the lines that are words someone said', () => {
    const text = '说话人 1  00:00\n我们用套瑞\n\n说话人 2  00:04\n好'
    const at = spokenLines(text, '说话人')
    expect(at).toEqual([1, 4])
    expect(withLines(text, at, ['我们用 Tauri。', '好。'])).toBe(
      '说话人 1  00:00\n我们用 Tauri。\n\n说话人 2  00:04\n好。',
    )
    expect(withLines(text, at, ['', undefined])).toBe(text)
  })

  it('marks a note with the time its line was begun at', () => {
    expect(stampNote('预', 1, 65)).toEqual({ value: '[01:05] 预', caret: 9 })
    // The line already has its time, and typing on does not add another.
    expect(stampNote('[01:05] 预算', 10, 90)).toEqual({ value: '[01:05] 预算', caret: 10 })
    expect(stampNote('[01:05] 预算\n排', 12, 130)).toEqual({
      value: '[01:05] 预算\n[02:10] 排',
      caret: 20,
    })
    expect(stampNote('', 0, 5)).toEqual({ value: '', caret: 0 })
  })

  it('finds the notes that can be looked up in the recording', () => {
    expect(stampedNotes('[01:05] 预算\n没有时间的一行\n[1:02:10] 排期\n[00:07] ')).toEqual([
      { seconds: 65, time: '01:05', text: '预算' },
      { seconds: 3730, time: '1:02:10', text: '排期' },
    ])
  })

  it('takes a line that only looks like a heading as words', () => {
    expect(transcriptLines('说话人 2 说了很多', '说话人')).toEqual([{ text: '说话人 2 说了很多' }])
  })
})
