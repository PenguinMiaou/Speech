import { beforeEach, describe, expect, it, vi } from 'vitest'

const meetingTranslate = vi.fn()
vi.mock('../../lib/tauri', () => ({
  meetingTranslate: (...args: unknown[]) => meetingTranslate(...args),
  meetingSave: vi.fn(),
  onMeetingError: vi.fn(),
  onMeetingInterim: vi.fn(),
  onMeetingNames: vi.fn(),
  onMeetingSegment: vi.fn(),
  speakerConfirm: vi.fn(),
}))

import { initialMeeting, takeSegment, useMeetingStore } from '../meetingStore'

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
})
