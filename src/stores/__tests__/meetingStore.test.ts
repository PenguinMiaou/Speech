import '../../i18n'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const meetingTranslate = vi.fn()
const meetingSpeakersAgain = vi.fn()
const meetingTitle = vi.fn()
const meetingSave = vi.fn()
const meetingRecover = vi.fn()
const meetingWordTimes = vi.fn()
let translatingSoFar: (text: string) => void = () => {}
vi.mock('../../lib/tauri', () => ({
  meetingTranslate: (...args: unknown[]) => meetingTranslate(...args),
  meetingSpeakersAgain: (...args: unknown[]) => meetingSpeakersAgain(...args),
  meetingTitle: (...args: unknown[]) => meetingTitle(...args),
  meetingSave: (...args: unknown[]) => meetingSave(...args),
  meetingRecover: (...args: unknown[]) => meetingRecover(...args),
  meetingWordTimes: (...args: unknown[]) => meetingWordTimes(...args),
  meetingAudioKeep: vi.fn(),
  onMeetingError: vi.fn(),
  onMeetingInterim: vi.fn(),
  onMeetingNames: vi.fn(),
  onMeetingSegment: vi.fn(),
  onMeetingTranslating: (handler: (text: string) => void) => {
    translatingSoFar = handler
  },
  speakerConfirm: vi.fn(),
}))

import {
  initialMeeting,
  listenToMeeting,
  nameFromMinutes,
  nameMeeting,
  persistMeeting,
  recoverMeeting,
  takeSegment,
  tellSpeakersAgain,
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
    meetingSpeakersAgain.mockReset()
    meetingTitle.mockReset()
    meetingSave.mockReset()
    useMeetingStore.setState({ ...recorded })
  })

  it('has its speakers told again from the whole sound, and is kept so', async () => {
    meetingSpeakersAgain.mockResolvedValue([
      { start_ms: 0, end_ms: 9_000, speaker: 5 },
      { start_ms: 20_000, end_ms: 26_000, speaker: 5 },
    ])
    await tellSpeakersAgain('Speaker')
    const state = useMeetingStore.getState()
    expect(meetingSpeakersAgain).toHaveBeenCalledWith('m-1')
    expect(state.text).toBe('Speaker 1  00:00\na\n\nSpeaker 1  00:20\nc')
    expect(state.paragraphs.map((each) => each.speaker)).toEqual([1, 1])
    expect(state.sorting).toBe(false)
    expect(meetingSave).toHaveBeenCalledWith(expect.objectContaining({ transcript: state.text }))
  })

  it('keeps its speakers when the sound was not kept or cannot be heard again', async () => {
    useMeetingStore.setState({ kept: null })
    await tellSpeakersAgain('Speaker')
    expect(meetingSpeakersAgain).not.toHaveBeenCalled()

    useMeetingStore.setState({ kept: 'm-1.wav' })
    meetingSpeakersAgain.mockRejectedValue('engine')
    await tellSpeakersAgain('Speaker')
    expect(useMeetingStore.getState()).toMatchObject({ text: recorded.text, sorting: false })
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
