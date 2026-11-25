import '../../i18n'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const meetingTranslate = vi.fn()
const meetingTitle = vi.fn()
const meetingSave = vi.fn()
const meetingRecover = vi.fn()
const meetingWordTimes = vi.fn()
const meetingAudioKeep = vi.fn()
const speakerVoicesKeep = vi.fn()
const meetingCorrect = vi.fn()
let packedSoFar: (packed: { id: string; path: string }) => void = () => {}
let translatingSoFar: (text: string) => void = () => {}
vi.mock('../../lib/tauri', () => ({
  meetingTranslate: (...args: unknown[]) => meetingTranslate(...args),
  meetingCorrect: (...args: unknown[]) => meetingCorrect(...args),
  meetingTitle: (...args: unknown[]) => meetingTitle(...args),
  meetingSave: (...args: unknown[]) => meetingSave(...args),
  meetingRecover: (...args: unknown[]) => meetingRecover(...args),
  meetingWordTimes: (...args: unknown[]) => meetingWordTimes(...args),
  meetingAudioKeep: (...args: unknown[]) => meetingAudioKeep(...args),
  onMeetingAudioPacked: (handler: (packed: { id: string; path: string }) => void) => {
    packedSoFar = handler
  },
  onFileTranscribeProgress: vi.fn(),
  onFileTranscribeWritten: vi.fn(),
  onMeetingPolishing: vi.fn(),
  onMeetingTerms: vi.fn(),
  onMeetingError: vi.fn(),
  onMeetingInterim: vi.fn(),
  onMeetingNames: vi.fn(),
  onMeetingSegment: vi.fn(),
  onMeetingTranslating: (handler: (text: string) => void) => {
    translatingSoFar = handler
  },
  speakerConfirm: vi.fn(),
  speakerVoicesKeep: (...args: unknown[]) => speakerVoicesKeep(...args),
}))

import {
  initialMeeting,
  listenToMeeting,
  nameFromMinutes,
  nameMeeting,
  persistMeeting,
  recoverMeeting,
  soundOf,
  takeSegment,
  timeWords,
  useMeetingStore,
} from '../meetingStore'

const said = (speaker: number, text: string) => ({ speaker, start_ms: 0, text })

describe('a piece of the meeting arriving', () => {
  beforeEach(() => {
    meetingTranslate.mockReset()
    useMeetingStore.setState({ ...initialMeeting })
  })

  it('takes the place of the guess that was shown for it', async () => {
    useMeetingStore.setState({ interim: '周五上' })
    await takeSegment(said(1, '周五上线'))
    expect(useMeetingStore.getState().interim).toBe('')
  })

  it('is not sent anywhere when translation is off', async () => {
    await takeSegment(said(1, '周五上线'))
    expect(meetingTranslate).not.toHaveBeenCalled()
    expect(useMeetingStore.getState().paragraphs[0].text).toBe('周五上线')
  })

  it('is translated into the chosen language, in the order it was said', async () => {
    useMeetingStore.setState({ translateTo: 'en' })
    let first: (text: string) => void = () => {}
    meetingTranslate
      .mockReturnValueOnce(new Promise<string>((done) => (first = done)))
      .mockResolvedValueOnce('Testing first.')
    const one = takeSegment(said(1, '周五上线'))
    const two = takeSegment(said(1, '先测试'))
    first('We ship on Friday.')
    await Promise.all([one, two])
    expect(meetingTranslate).toHaveBeenNthCalledWith(1, '周五上线', 'en')
    expect(useMeetingStore.getState().paragraphs[0].translation).toBe(
      'We ship on Friday. Testing first.',
    )
  })

  it('keeps the words and says so when a translation fails', async () => {
    useMeetingStore.setState({ translateTo: 'en' })
    meetingTranslate.mockRejectedValueOnce('llm_not_configured')
    await takeSegment(said(1, '周五上线'))
    const state = useMeetingStore.getState()
    expect(state.paragraphs[0].text).toBe('周五上线')
    expect(state.error).toBe('meeting.translateFailed')
  })

  it('shows a translation as it is written, and only its whole once it is done', async () => {
    listenToMeeting()
    useMeetingStore.setState({ translateTo: 'en' })
    let done: (text: string) => void = () => {}
    meetingTranslate.mockReturnValueOnce(new Promise<string>((resolve) => (done = resolve)))
    const taken = takeSegment(said(1, '周五上线'))
    await Promise.resolve()

    translatingSoFar('We ship')
    expect(useMeetingStore.getState()).toMatchObject({ translatingAt: 0, translating: 'We ship' })

    done('We ship on Friday.')
    await taken
    const state = useMeetingStore.getState()
    expect(state.paragraphs[0].translation).toBe('We ship on Friday.')
    expect(state).toMatchObject({ translatingAt: null, translating: '' })
    translatingSoFar('late')
    expect(useMeetingStore.getState().translating).toBe('')
  })
})

describe('words put right while the meeting is recorded', () => {
  beforeEach(() => {
    meetingTranslate.mockReset()
    meetingCorrect.mockReset()
    useMeetingStore.setState({
      ...initialMeeting,
      phase: 'recording',
      liveFix: true,
      people: ['Bo Chen'],
      names: { 2: 'Ann' },
    })
  })
  const settle = () => new Promise((done) => setTimeout(done, 0))

  it('puts right the paragraphs that will not grow, keeping what was heard', async () => {
    meetingCorrect.mockImplementation(async (pieces: string[]) =>
      pieces.map((piece) => piece.replace('bow', 'Bo')),
    )
    await takeSegment(said(1, 'bow said yes'))
    await takeSegment(said(2, 'fine'))
    await takeSegment(said(1, 'then bow left'))
    // The newest paragraph may still grow, so it waits.
    expect(meetingCorrect).not.toHaveBeenCalled()
    await takeSegment(said(2, 'ok'))
    await settle()
    expect(meetingCorrect).toHaveBeenCalledWith(
      ['bow said yes', 'fine', 'then bow left'],
      ['Bo Chen', 'Ann'],
      '',
    )
    const [first, , third, last] = useMeetingStore.getState().paragraphs
    expect(first).toMatchObject({ text: 'Bo said yes', heard: 'bow said yes' })
    expect(third).toMatchObject({ text: 'then Bo left', heard: 'then bow left' })
    expect(last.heard).toBeUndefined()
  })

  it('leaves alone a paragraph the user is changing or changed meanwhile', async () => {
    let answer: (pieces: string[]) => void = () => {}
    meetingCorrect.mockReturnValue(new Promise((done) => (answer = done)))
    for (const [speaker, text] of [
      [1, 'one bow'],
      [2, 'two bow'],
      [1, 'three bow'],
      [2, 'four'],
    ] as const) {
      await takeSegment(said(speaker, text))
    }
    useMeetingStore.setState(({ paragraphs }) => ({
      paragraphs: paragraphs.map((paragraph, index) =>
        index === 1 ? { ...paragraph, text: 'two, typed' } : paragraph,
      ),
    }))
    answer(['one Bo', 'two Bo', 'three Bo'])
    await settle()
    expect(useMeetingStore.getState().paragraphs.map((paragraph) => paragraph.text)).toEqual([
      'one Bo',
      'two, typed',
      'three Bo',
      'four',
    ])
  })

  it('sends nothing when it is not asked for', async () => {
    useMeetingStore.setState({ liveFix: false })
    for (const [speaker, text] of [
      [1, 'a'],
      [2, 'b'],
      [1, 'c'],
      [2, 'd'],
    ] as const) {
      await takeSegment(said(speaker, text))
    }
    expect(meetingCorrect).not.toHaveBeenCalled()
  })
})

describe('a meeting that has been recorded', () => {
  const recorded = {
    ...initialMeeting,
    id: 'm-1',
    recorded: true,
    kept: 'm-1.wav',
    paragraphs: [
      { speaker: 1, startMs: 0, text: 'a' },
      { speaker: 3, startMs: 20_000, text: 'c' },
    ],
    text: 'Speaker 1  00:00\na\n\nSpeaker 3  00:20\nc',
  }

  beforeEach(() => {
    meetingTitle.mockReset()
    meetingSave.mockReset()
    useMeetingStore.setState({ ...recorded })
  })

  it('keeps the voices just heard with the meeting, and only those', async () => {
    speakerVoicesKeep.mockReset()
    useMeetingStore.setState({ voicesAtHand: true })
    await persistMeeting('Speaker')
    expect(speakerVoicesKeep).toHaveBeenCalledWith('m-1')
    speakerVoicesKeep.mockReset()
    useMeetingStore.setState({ voicesAtHand: false })
    await persistMeeting('Speaker')
    expect(speakerVoicesKeep).not.toHaveBeenCalled()
  })

  it('is named again after its minutes, unless the user named it', async () => {
    meetingTitle.mockResolvedValueOnce('First reading')
    await nameMeeting('Speaker')
    expect(useMeetingStore.getState().title).toBe('First reading')

    meetingTitle.mockResolvedValueOnce('Budget cut agreed')
    await nameFromMinutes('Speaker', 'the minutes')
    expect(meetingTitle).toHaveBeenLastCalledWith('the minutes')
    expect(useMeetingStore.getState().title).toBe('Budget cut agreed')

    useMeetingStore.setState({ title: 'My own name' })
    await nameFromMinutes('Speaker', 'the minutes')
    expect(meetingTitle).toHaveBeenCalledTimes(2)
    expect(useMeetingStore.getState().title).toBe('My own name')
  })
})

describe('a recording that was cut short', () => {
  it('is brought back once, with what was written down and the notes typed during it', async () => {
    useMeetingStore.setState({ ...initialMeeting, phase: 'recording' })
    useMeetingStore.setState({ notes: '[00:03] cut the budget' })
    useMeetingStore.setState({ ...initialMeeting })
    meetingRecover.mockResolvedValue({
      segments: [
        { speaker: 1, start_ms: 0, text: 'one' },
        { speaker: 2, start_ms: 4_000, text: 'two' },
      ],
      seconds: 9,
    })

    expect(await recoverMeeting('Speaker')).toBe(true)

    expect(useMeetingStore.getState()).toMatchObject({
      text: 'Speaker 1  00:00\none\n\nSpeaker 2  00:04\ntwo',
      recorded: true,
      duration: 9,
      notes: '[00:03] cut the budget',
    })
    expect(await recoverMeeting('Speaker')).toBe(false)
    expect(meetingRecover).toHaveBeenCalledTimes(1)
  })
})

describe('the user among the speakers', () => {
  beforeEach(() => useMeetingStore.setState({ ...initialMeeting }))
  const piece = (speaker: number, mine: boolean) => ({
    speaker,
    start_ms: 0,
    text: '这一段话有十个字左右',
    mine,
  })

  it('is named once the microphone and the computer tell them apart', async () => {
    for (const each of [piece(2, true), piece(1, false), piece(2, true)]) await takeSegment(each)
    expect(useMeetingStore.getState().names).toEqual({ 2: 'Me' })
  })

  it('keeps a name the user typed', async () => {
    useMeetingStore.setState({ names: { 2: 'Lin' } })
    for (const each of [piece(2, true), piece(1, false), piece(2, true)]) await takeSegment(each)
    expect(useMeetingStore.getState().names).toEqual({ 2: 'Lin' })
  })

  it('takes the name back when it turns out to be someone else', async () => {
    for (const each of [piece(2, true), piece(1, false), piece(2, true)]) await takeSegment(each)
    for (let times = 0; times < 4; times++) await takeSegment(piece(2, false))
    expect(useMeetingStore.getState().names).toEqual({})
  })
})

describe('when the words of a meeting were said', () => {
  it('is kept with the meeting', async () => {
    const times = [{ end_ms: 8000, chars: 4, marks: [[0, 0]] as [number, number][] }]
    useMeetingStore.setState({ ...initialMeeting, text: 'Speaker 1  00:00\n周五上线', times })
    await persistMeeting('Speaker')
    expect(meetingSave).toHaveBeenLastCalledWith(expect.objectContaining({ times }))
  })

  it('keeps the overview with the meeting when it is saved for something else', async () => {
    const overview = {
      chars: 14,
      chapters: [{ at_s: 0, title: 'Launch', summary: 'Friday.' }],
      speakers: [],
    }
    useMeetingStore.setState({ ...initialMeeting, text: 'Speaker 1  00:00\n周五上线', overview })
    await persistMeeting('Speaker')
    expect(meetingSave).toHaveBeenLastCalledWith(expect.objectContaining({ overview }))
  })
})

describe('the times of the words of a meeting just recorded', () => {
  const text = 'Speaker 1  00:10\n周五上线'
  const before = [{ end_ms: 18000, chars: 4, marks: [[0, 10000]] as [number, number][] }]
  beforeEach(() => {
    meetingWordTimes.mockReset()
    meetingSave.mockReset()
    useMeetingStore.setState({
      ...initialMeeting,
      id: 'm-1',
      recorded: true,
      kept: '/kept.wav',
      text,
      times: before,
      duration: 20,
    })
  })

  it('are heard from the kept sound and kept with the meeting', async () => {
    meetingWordTimes.mockResolvedValue([[[2, 12000]]])
    await timeWords('Speaker')
    expect(meetingWordTimes).toHaveBeenCalledWith('m-1', [
      { start_ms: 10000, end_ms: 18000, text: '周五上线' },
    ])
    const times = [
      {
        end_ms: 18000,
        chars: 4,
        marks: [
          [0, 10000],
          [2, 12000],
        ],
      },
    ]
    expect(useMeetingStore.getState().times).toEqual(times)
    expect(meetingSave).toHaveBeenCalledWith(expect.objectContaining({ times }))
  })

  it('stay as they were when they cannot be heard, as without the model', async () => {
    meetingWordTimes.mockRejectedValue('word_times_not_installed')
    await timeWords('Speaker')
    expect(useMeetingStore.getState().times).toEqual(before)
    expect(meetingSave).not.toHaveBeenCalled()
  })

  it('are left alone when the words were changed meanwhile', async () => {
    let found: (marks: [number, number][][]) => void = () => {}
    meetingWordTimes.mockReturnValue(new Promise((resolve) => (found = resolve)))
    const asked = timeWords('Speaker')
    useMeetingStore.setState({ text: 'Speaker 1  00:10\n周五不上线' })
    found([[[2, 12000]]])
    await asked
    expect(useMeetingStore.getState().times).toEqual(before)
  })
})

describe('the sound of a meeting packed beside the window', () => {
  const recorded = {
    ...initialMeeting,
    id: 'm-7',
    recorded: true,
    kept: null,
    paragraphs: [{ speaker: 1, startMs: 0, text: 'a' }],
    text: 'Speaker 1  00:00\na',
  }

  beforeEach(() => {
    listenToMeeting()
    meetingSave.mockReset().mockResolvedValue(undefined)
    useMeetingStore.setState({ ...recorded })
  })

  it('is played from the packed file even when packing finishes before the WAV is told', async () => {
    let tell: (path: string) => void = () => {}
    meetingAudioKeep.mockReturnValueOnce(new Promise<string>((resolve) => (tell = resolve)))
    const saving = persistMeeting('Speaker')
    await vi.waitFor(() => expect(meetingAudioKeep).toHaveBeenCalledWith('m-7'))
    packedSoFar({ id: 'm-7', path: '/data/meetings/m-7.flac' })
    tell('/data/meetings/m-7.wav')
    await saving
    expect(useMeetingStore.getState().kept).toBe('/data/meetings/m-7.flac')
    expect(soundOf('m-7', '/data/meetings/m-7.wav')).toBe('/data/meetings/m-7.flac')
    expect(soundOf('m-8', '/data/meetings/m-8.wav')).toBe('/data/meetings/m-8.wav')
  })

  it('moves the open meeting to the packed file when packing finishes after', async () => {
    useMeetingStore.setState({ id: 'm-9', kept: '/data/meetings/m-9.wav' })
    packedSoFar({ id: 'm-9', path: '/data/meetings/m-9.flac' })
    expect(useMeetingStore.getState().kept).toBe('/data/meetings/m-9.flac')
    // Another meeting's sound is left alone.
    packedSoFar({ id: 'm-10', path: '/data/meetings/m-10.flac' })
    expect(useMeetingStore.getState().kept).toBe('/data/meetings/m-9.flac')
  })
})
