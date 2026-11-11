import { beforeEach, describe, expect, it, vi } from 'vitest'

const meetingTranslate = vi.fn()
const meetingSpeakersAgain = vi.fn()
const meetingTitle = vi.fn()
const meetingSave = vi.fn()
let translatingSoFar: (text: string) => void = () => {}
vi.mock('../../lib/tauri', () => ({
  meetingTranslate: (...args: unknown[]) => meetingTranslate(...args),
  meetingSpeakersAgain: (...args: unknown[]) => meetingSpeakersAgain(...args),
  meetingTitle: (...args: unknown[]) => meetingTitle(...args),
  meetingSave: (...args: unknown[]) => meetingSave(...args),
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
  takeSegment,
  tellSpeakersAgain,
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
