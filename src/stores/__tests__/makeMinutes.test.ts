import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as tauri from '../../lib/tauri'
import { initialMeeting, makeMinutes, useMeetingStore } from '../meetingStore'

vi.mock('../../lib/tauri')

const kept = {
  id: 'm-1',
  started_at: 1,
  transcript: 'S 1  00:00\num we ship friday\n\nS 2  00:05\nok',
  notes: 'a note',
  minutes: '',
  names: { 1: 'Ann' },
  title: 'Launch',
  duration_s: 9,
  times: [],
}
const options = { speakerLabel: 'S', modelId: null, languages: [] }

describe('minutes made of a kept meeting in one go', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useMeetingStore.setState(initialMeeting)
    vi.mocked(tauri.meetingGet).mockResolvedValue({ ...kept })
    vi.mocked(tauri.meetingSave).mockResolvedValue()
    vi.mocked(tauri.meetingPolish).mockResolvedValue(['We ship Friday.', 'OK.'])
    vi.mocked(tauri.meetingSpeakersCheck).mockImplementation(async (pieces) => pieces)
    vi.mocked(tauri.meetingMinutes).mockResolvedValue('# Decided\nShip Friday')
  })

  it('puts the words right, checks the speakers, writes the minutes and keeps it all', async () => {
    const steps: string[] = []
    const off = useMeetingStore.subscribe(({ making }) => {
      const step = making?.steps[making.at]
      if (step && steps[steps.length - 1] !== step) steps.push(step)
    })
    await makeMinutes('S', 'm-1', 'the template', null)
    off()
    expect(steps).toEqual(['words', 'speakers', 'minutes'])
    expect(tauri.transcribeFile).not.toHaveBeenCalled()
    expect(tauri.meetingPolish).toHaveBeenCalledWith(
      ['um we ship friday', 'ok'],
      expect.stringContaining('The meeting was held on 1970-01-01'),
    )
    // The minutes are written of the words as put right, under the speakers' names.
    expect(tauri.meetingMinutes).toHaveBeenCalledWith(
      'Ann  00:00\nWe ship Friday.\n\nS 2  00:05\nOK.',
      'the template',
      'a note',
    )
    expect(vi.mocked(tauri.meetingSave).mock.lastCall?.[0]).toMatchObject({
      id: 'm-1',
      transcript: 'S 1  00:00\nWe ship Friday.\n\nS 2  00:05\nOK.',
      minutes: '# Decided\nShip Friday',
      title: 'Launch',
    })
    expect(useMeetingStore.getState().making).toBeNull()
  })

  it('writes the kept sound out anew first when asked to', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('S 1  00:00\nwe ship friday')
    vi.mocked(tauri.meetingPolish).mockResolvedValue(['We ship Friday.'])
    const steps: string[] = []
    const off = useMeetingStore.subscribe(({ making }) => {
      const step = making?.steps[making.at]
      if (step && steps[steps.length - 1] !== step) steps.push(step)
    })
    await makeMinutes('S', 'm-1', 'the template', { audio: '/kept/m-1.wav', options })
    off()
    expect(steps).toEqual(['text', 'words', 'speakers', 'minutes'])
    expect(tauri.transcribeFile).toHaveBeenCalledWith('/kept/m-1.wav', options)
    expect(tauri.meetingPolish).toHaveBeenCalledWith(['we ship friday'], expect.any(String))
    expect(vi.mocked(tauri.meetingSave).mock.lastCall?.[0]).toMatchObject({
      transcript: 'S 1  00:00\nWe ship Friday.',
      minutes: '# Decided\nShip Friday',
      times: [],
    })
  })

  it('keeps the words that were put right when the minutes could not be written', async () => {
    vi.mocked(tauri.meetingMinutes).mockRejectedValue('llm_not_configured')
    await makeMinutes('S', 'm-1', 'the template', null)
    expect(vi.mocked(tauri.meetingSave).mock.lastCall?.[0]).toMatchObject({
      transcript: 'S 1  00:00\nWe ship Friday.\n\nS 2  00:05\nOK.',
      minutes: '',
    })
    expect(useMeetingStore.getState().error).toBe('meeting.error.llm_not_configured')
    expect(useMeetingStore.getState().making).toBeNull()
  })

  it('writes minutes of the words as heard when they could not be put right', async () => {
    vi.mocked(tauri.meetingPolish).mockRejectedValue('timeout')
    await makeMinutes('S', 'm-1', 'the template', null)
    expect(tauri.meetingMinutes).toHaveBeenCalledWith(
      'Ann  00:00\num we ship friday\n\nS 2  00:05\nok',
      'the template',
      'a note',
    )
  })

  it('shows its result in the meeting that is open', async () => {
    useMeetingStore.setState({ id: 'm-1', text: kept.transcript })
    await makeMinutes('S', 'm-1', 'the template', null)
    const now = useMeetingStore.getState()
    expect(now.text).toBe('S 1  00:00\nWe ship Friday.\n\nS 2  00:05\nOK.')
    expect(now.minutes).toBe('# Decided\nShip Friday')
    expect(now.unpolished).toEqual({ id: 'm-1', text: kept.transcript })
  })

  it('begins a paragraph anew where the words put right mark another subject', async () => {
    useMeetingStore.setState({ id: 'm-1', text: kept.transcript })
    vi.mocked(tauri.meetingPolish).mockResolvedValue(['We ship Friday. ¶ Next the budget.', 'OK.'])
    await makeMinutes('S', 'm-1', 'the template', null)
    const split = 'S 1  00:00\nWe ship Friday.\n\nS 1  00:02\nNext the budget.\n\nS 2  00:05\nOK.'
    // Who said what is checked of the paragraphs as they now stand.
    expect(tauri.meetingSpeakersCheck).toHaveBeenCalledWith([
      { speaker: 1, text: 'We ship Friday.' },
      { speaker: 1, text: 'Next the budget.' },
      { speaker: 2, text: 'OK.' },
    ])
    expect(vi.mocked(tauri.meetingSave).mock.lastCall?.[0]).toMatchObject({ transcript: split })
    const now = useMeetingStore.getState()
    expect(now.text).toBe(split)
    expect(now.unpolished?.text).toBe(kept.transcript)
    expect(now.unpolished?.shaped?.split('\n')).toHaveLength(split.split('\n').length)
  })

  it('does one meeting at a time', async () => {
    const first = makeMinutes('S', 'm-1', 'the template', null)
    await makeMinutes('S', 'm-2', 'the template', null)
    await first
    expect(tauri.meetingGet).toHaveBeenCalledTimes(1)
  })
})
