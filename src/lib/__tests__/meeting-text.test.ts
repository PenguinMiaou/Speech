import { describe, expect, it } from 'vitest'
import {
  addSegment,
  addTranslation,
  charsSaid,
  meetingText,
  renameSpeakers,
  secondsAt,
  speakersAgain,
  speakersIn,
  spansOf,
  spokenLines,
  stampNote,
  stampedNotes,
  subtitles,
  timesOf,
  transcriptLines,
  whoIsMe,
  withLines,
  withMarks,
  withParagraphText,
  withSide,
  withSpeakers,
  type Paragraph,
  type Sides,
  remaining,
  saidBy,
  speakersOf,
  withSpeakerAt,
  withSpeakersChecked,
  withSpeakersJoined,
  withSpellings,
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
    expect(paragraphs).toMatchObject([
      { speaker: 1, startMs: 0, text: '你好' },
      { speaker: 2, startMs: 4_000, text: '收到' },
    ])
  })

  it('lets a speaker go on across two windows in one paragraph', () => {
    let paragraphs = addSegment([], said(1, 0, 'the first half'))
    paragraphs = addSegment(paragraphs, said(1, 12_000, 'and the rest'))
    expect(paragraphs).toMatchObject([
      { speaker: 1, startMs: 0, text: 'the first half and the rest' },
    ])
  })

  it('gives a voice too short to tell to whoever spoke last', () => {
    let paragraphs = addSegment([], said(2, 0, '我觉得可以'))
    paragraphs = addSegment(paragraphs, said(0, 5_000, '对'))
    expect(paragraphs).toMatchObject([{ speaker: 2, startMs: 0, text: '我觉得可以对' }])
  })

  it('keeps each window its own paragraph when speakers are not told apart', () => {
    let paragraphs = addSegment([], said(null, 0, 'one'))
    paragraphs = addSegment(paragraphs, said(null, 30_000, 'two'))
    paragraphs = addSegment(paragraphs, said(null, 60_000, '  '))
    expect(meetingText(paragraphs, 'Speaker')).toBe('00:00\none\n\n00:30\ntwo')
  })

  it('joins what follows soon when speakers are not told apart', () => {
    let paragraphs = addSegment([], said(null, 0, 'one'))
    paragraphs = addSegment(paragraphs, said(null, 8_000, 'two'))
    expect(paragraphs).toMatchObject([{ speaker: null, startMs: 0, text: 'one two' }])
  })

  it('reads a time on a line of its own as the moment the words under it were said', () => {
    expect(transcriptLines('00:30\ntwo\n\n1:02:05\nthree\n12:30 is late', 'Speaker')).toEqual([
      { speaker: null, time: '00:30' },
      { text: 'two' },
      { text: '' },
      { speaker: null, time: '1:02:05' },
      { text: 'three' },
      { text: '12:30 is late' },
    ])
  })

  it('tells again who spoke each paragraph, keeping the numbers people already have', () => {
    const paragraphs = [
      { speaker: 1, startMs: 0, text: 'a' },
      { speaker: 2, startMs: 10_000, text: 'b' },
      // Heard as a third person while recording, and as the first in the whole sound.
      { speaker: 3, startMs: 20_000, text: 'c' },
      { speaker: 2, startMs: 30_000, text: 'd' },
      // Someone the whole sound tells apart from the others.
      { speaker: 2, startMs: 40_000, text: 'e' },
    ]
    const turn = (start_ms: number, end_ms: number, speaker: number) => ({
      start_ms,
      end_ms,
      speaker,
    })
    const turns = [
      turn(0, 9_000, 7),
      turn(10_000, 19_000, 8),
      turn(20_000, 26_000, 7),
      turn(30_000, 39_000, 8),
      turn(40_000, 45_000, 9),
    ]
    expect(speakersAgain(paragraphs, turns)).toEqual([1, 2, 1, 2, 4])
  })

  it('leaves a paragraph nobody was heard in, and a meeting without speakers, as they are', () => {
    const turns = [{ start_ms: 0, end_ms: 5_000, speaker: 1 }]
    expect(
      speakersAgain(
        [
          { speaker: 2, startMs: 0, text: 'a' },
          { speaker: 0, startMs: 60_000, text: 'b' },
        ],
        turns,
      ),
    ).toEqual([2, 0])
    expect(speakersAgain([{ speaker: null, startMs: 0, text: 'a' }], turns)).toEqual([null])
  })

  it('puts the speakers told again into the headings of a text', () => {
    const text = 'Speaker 1  00:00\na\n\nSpeaker 3  00:20\nSpeaker 3 said\n\nSpeaker  00:40\nc'
    expect(withSpeakers(text, 'Speaker', [1, 1, 2])).toBe(
      'Speaker 1  00:00\na\n\nSpeaker 1  00:20\nSpeaker 3 said\n\nSpeaker 2  00:40\nc',
    )
    // A paragraph was removed meanwhile: the headings no longer answer the paragraphs.
    expect(withSpeakers(text, 'Speaker', [1, 1])).toBe(text)
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

describe('telling which speaker is the user', () => {
  const piece = (speaker: number | null, mine: boolean | null, text = '这一段话有十个字左右') => ({
    speaker,
    start_ms: 0,
    text,
    mine,
  })
  const after = (...pieces: ReturnType<typeof piece>[]) =>
    pieces.reduce((sides, each) => withSide(sides, each), {} as Sides)

  it('is the one heard by the microphone while the others come from the computer', () => {
    const sides = after(piece(2, true), piece(1, false), piece(2, true), piece(1, false))
    expect(whoIsMe(sides)).toBe(2)
  })

  it('is nobody when the computer plays no one, as in a room', () => {
    expect(whoIsMe(after(piece(1, true), piece(2, true), piece(1, true)))).toBeNull()
  })

  it('is nobody when two people share the microphone', () => {
    const sides = after(
      piece(1, true),
      piece(2, true),
      piece(3, false),
      piece(1, true),
      piece(2, true),
    )
    expect(whoIsMe(sides)).toBeNull()
  })

  it('is nobody on a few words only', () => {
    expect(whoIsMe(after(piece(2, true, '好的'), piece(1, false)))).toBeNull()
  })

  it('leaves out pieces whose side or speaker is not known', () => {
    const sides = after(piece(null, true), piece(0, true), piece(1, null))
    expect(sides).toEqual({})
  })
})

describe('when the words inside a paragraph were said', () => {
  const heard = [
    { speaker: 1, start_ms: 0, end_ms: 4000, text: '周五上线' },
    { speaker: 1, start_ms: 5000, end_ms: 8000, text: '先测试' },
  ].reduce<Paragraph[]>((all, each) => addSegment(all, each), [])
  const [times] = timesOf(heard)

  it('is marked where each piece joined the paragraph', () => {
    expect(heard).toHaveLength(1)
    expect(times).toEqual({
      end_ms: 8000,
      chars: 7,
      marks: [
        [0, 0],
        [4, 5000],
      ],
    })
  })

  it('gives a place in the words its time', () => {
    expect(secondsAt(times, 7, 2)).toBe(2.5)
    expect(secondsAt(times, 7, 4)).toBe(5)
    expect(secondsAt(times, 7, 7)).toBe(8)
  })

  it('gives a time the words said by then', () => {
    expect(charsSaid(times, 7, 2.5)).toBe(2)
    expect(charsSaid(times, 7, 6.5)).toBe(5)
    expect(charsSaid(times, 7, 60)).toBe(7)
  })

  it('still fits words whose length was changed since', () => {
    expect(secondsAt(times, 14, 8)).toBe(5)
    expect(charsSaid(times, 14, 5)).toBe(8)
  })

  it('has nothing to say of a piece whose end is not known', () => {
    const old = addSegment([], { speaker: 1, start_ms: 3000, text: '周五上线' })
    expect(secondsAt(timesOf(old)[0], 4, 2)).toBeNull()
  })
})

describe('a meeting as subtitles', () => {
  const text = 'Speaker 1  00:10\n周五上线先测试\n\nSpeaker 2  00:20\n好的\n\n01:05\nlast words'
  const times = [
    {
      end_ms: 18000,
      chars: 7,
      marks: [
        [0, 10000],
        [4, 15000],
      ] as [number, number][],
    },
    { end_ms: 21500, chars: 2, marks: [[0, 20000]] as [number, number][] },
    { end_ms: 0, chars: 10, marks: [[0, 65000]] as [number, number][] },
  ]

  it('gives each piece its own lines with the times it was said between', () => {
    expect(subtitles(text, 'Speaker', { 2: 'Lin' }, times, 'srt')).toBe(
      [
        '1\n00:00:10,000 --> 00:00:15,000\nSpeaker 1: 周五上线',
        '2\n00:00:15,000 --> 00:00:18,000\nSpeaker 1: 先测试',
        '3\n00:00:20,000 --> 00:00:21,500\nLin: 好的',
        '4\n00:01:05,000 --> 00:01:10,000\nlast words',
      ].join('\n\n') + '\n',
    )
  })

  it('ends a piece whose end is not known where the next one begins', () => {
    const written = subtitles(text, 'Speaker', {}, [], 'vtt')
    expect(
      written.startsWith('WEBVTT\n\n00:00:10.000 --> 00:00:20.000\n<v Speaker 1>周五上线先测试\n'),
    ).toBe(true)
    expect(written).toContain('00:00:20.000 --> 00:01:05.000\n<v Speaker 2>好的')
  })

  it('is empty for a text without times', () => {
    expect(subtitles('just words', 'Speaker', {}, [], 'srt')).toBe('')
  })
})

describe('the words of a meeting heard again for their times', () => {
  const text = 'Speaker 1  00:10\n周五上线\n\nSpeaker 2  00:20\n好的\n\n01:05\nlast words'
  const times = [
    { end_ms: 18000, chars: 4, marks: [[0, 10400]] as [number, number][] },
    { end_ms: 0, chars: 2, marks: [[0, 20000]] as [number, number][] },
    { end_ms: 0, chars: 10, marks: [[0, 65000]] as [number, number][] },
  ]

  it('are given with where the sound of each paragraph begins and ends', () => {
    expect(spansOf(text, 'Speaker', times, 70)).toEqual([
      { start_ms: 10400, end_ms: 18000, text: '周五上线' },
      { start_ms: 20000, end_ms: 65000, text: '好的' },
      { start_ms: 65000, end_ms: 70000, text: 'last words' },
    ])
  })

  it('go by the headings alone where no times were kept', () => {
    expect(spansOf(text, 'Speaker', [], 0).map((span) => [span.start_ms, span.end_ms])).toEqual([
      [10000, 20000],
      [20000, 65000],
      [65000, 65000],
    ])
  })

  it('take the times found, and keep what they had where none were', () => {
    const found: [number, number][][] = [
      [
        [1, 11000],
        [3, 12000],
      ],
      [],
      [[0, 65200]],
    ]
    expect(withMarks(text, 'Speaker', times, 70, found)).toEqual([
      {
        end_ms: 18000,
        chars: 4,
        marks: [
          [0, 10400],
          [1, 11000],
          [3, 12000],
        ],
      },
      times[1],
      { end_ms: 70000, chars: 10, marks: [[0, 65200]] },
    ])
  })
})

describe('withSpeakerAt', () => {
  const text = 'S 1  00:10\na\n\nS 2  00:20\nb\n\nS 2  00:30\nc\n\nS  00:40\nd'

  it('gives one paragraph to another speaker', () => {
    expect(withSpeakerAt(text, 'S', 3, 1, false)).toBe(
      'S 1  00:10\na\n\nS 1  00:20\nb\n\nS 2  00:30\nc\n\nS  00:40\nd',
    )
  })

  it('gives all that speaker said to another', () => {
    expect(withSpeakerAt(text, 'S', 3, 1, true)).toBe(
      'S 1  00:10\na\n\nS 1  00:20\nb\n\nS 1  00:30\nc\n\nS  00:40\nd',
    )
  })

  it('leaves the text alone where the line is no heading', () => {
    expect(withSpeakerAt(text, 'S', 1, 2, true)).toBe(text)
  })
})

describe('withSpeakersChecked', () => {
  const text = 'S 1  00:10\na b\n\nS 2  00:20\nc d\n\n00:30\ne'

  it('tells who said each spoken line', () => {
    expect(speakersOf(text, 'S')).toEqual([1, 2, null])
  })

  it('gives the lines their checked speakers and words', () => {
    const checked = [
      { speaker: 1, text: 'a b c' },
      { speaker: 1, text: 'd' },
      { speaker: 0, text: 'e' },
    ]
    expect(withSpeakersChecked(text, 'S', checked)).toBe(
      'S 1  00:10\na b c\n\nS 1  00:20\nd\n\n00:30\ne',
    )
  })

  it('leaves a line alone that came back empty or not at all', () => {
    expect(withSpeakersChecked(text, 'S', [{ speaker: 1, text: '' }])).toBe(text)
  })
})

describe('remaining', () => {
  it('tells how long the rest takes at the pace so far', () => {
    expect(remaining(0, 0.25, 60_000)).toBe(180)
    expect(remaining(0, 0.5, 60_000)).toBe(60)
  })

  it('does not guess from next to nothing, nor once all is done', () => {
    expect(remaining(0, 0.01, 60_000)).toBeNull()
    expect(remaining(0, 1, 60_000)).toBeNull()
  })
})

describe('fewer speakers', () => {
  const text = 'S 1  00:10\nabcd\nef\n\nS 2  00:20\ngh\n\nS 1  00:30\ni\n\nS  00:40\njk\n\n00:50\nl'

  it('counts what each numbered speaker said', () => {
    expect(saidBy(text, 'S')).toEqual({ 1: 7, 2: 2 })
  })

  it('gives all a speaker said to the one they are taken for', () => {
    expect(
      withSpeakersJoined(text, 'S', [
        [2, 1],
        [5, 1],
      ]),
    ).toBe('S 1  00:10\nabcd\nef\n\nS 1  00:20\ngh\n\nS 1  00:30\ni\n\nS  00:40\njk\n\n00:50\nl')
  })
})

describe('withSpellings', () => {
  it('writes a term one way wherever it was said', () => {
    const text = 'S 1  00:10\nwe use Tori and tauri\n\nS 2  00:20\n套瑞很好，Tori 也是'
    expect(
      withSpellings(text, 'S', [{ spelling: 'Tauri', others: ['Tori', 'tauri', '套瑞'] }]),
    ).toBe('S 1  00:10\nwe use Tauri and Tauri\n\nS 2  00:20\nTauri很好，Tauri 也是')
  })

  it('leaves a longer word that holds the spelling, and the headings, alone', () => {
    const text = 'Art 1  00:10\nArt met Arthur, art'
    expect(withSpellings(text, 'Art', [{ spelling: 'Bart', others: ['Art', 'art'] }])).toBe(
      'Art 1  00:10\nBart met Arthur, Bart',
    )
  })
})
