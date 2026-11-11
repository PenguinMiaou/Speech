import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { MeetingCard } from '../MeetingCard'
import * as tauri from '../../../lib/tauri'
import { initialMeeting, useMeetingStore } from '../../../stores/meetingStore'

vi.mock('../../../lib/tauri')
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, unknown>) =>
      values ? `${key} ${Object.values(values).join(' ')}` : key,
  }),
}))

let drop: (paths: string[]) => void = () => {}
let report: (progress: tauri.FileTranscribeProgress) => void = () => {}
let hear: (segment: tauri.MeetingSegment) => void = () => {}
let write: (text: string) => void = () => {}
let name: (names: Record<string, string>) => void = () => {}

function installed(speakers: boolean) {
  vi.mocked(tauri.speakerModelsStatus).mockResolvedValue({
    installed: speakers,
    downloading: false,
    total_bytes: 34_000_000,
  })
}

async function ready() {
  render(<MeetingCard />)
  await waitFor(() => expect(tauri.onFileDropped).toHaveBeenCalled())
  await waitFor(() => expect(tauri.speakerModelsStatus).toHaveBeenCalled())
}

describe('MeetingCard', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    window.localStorage.clear()
    useMeetingStore.setState(initialMeeting)
    vi.mocked(tauri.onFileDropped).mockImplementation(async (handler) => {
      drop = handler
      return () => {}
    })
    vi.mocked(tauri.onFileTranscribeProgress).mockImplementation(async (handler) => {
      report = handler
      return () => {}
    })
    vi.mocked(tauri.onMeetingSegment).mockImplementation(async (handler) => {
      hear = handler
      return () => {}
    })
    vi.mocked(tauri.onFileTranscribeWritten).mockImplementation(async (handler) => {
      write = handler
      return () => {}
    })
    vi.mocked(tauri.onMeetingError).mockResolvedValue(() => {})
    vi.mocked(tauri.onMeetingInterim).mockResolvedValue(() => {})
    vi.mocked(tauri.onMeetingNames).mockImplementation(async (handler) => {
      name = handler
      return () => {}
    })
    vi.mocked(tauri.speakerVoices).mockResolvedValue([])
    vi.mocked(tauri.meetingsList).mockResolvedValue([])
    vi.mocked(tauri.meetingTitle).mockResolvedValue('')
    vi.mocked(tauri.meetingSave).mockResolvedValue()
    vi.mocked(tauri.meetingAudioKeep).mockResolvedValue('/data/meetings/m.wav')
    vi.mocked(tauri.meetingAudio).mockResolvedValue(null)
    vi.mocked(tauri.audioSource).mockImplementation((path) => path)
    vi.mocked(tauri.minutesTemplates).mockResolvedValue([])
    vi.mocked(tauri.onSpeakerModelsProgress).mockResolvedValue(() => {})
    vi.mocked(tauri.listLocalModels).mockResolvedValue([])
    installed(false)
  })
  afterEach(cleanup)

  it('shows progress, then the text of a dropped file', async () => {
    let finish: (text: string) => void = () => {}
    vi.mocked(tauri.transcribeFile).mockReturnValue(new Promise((resolve) => (finish = resolve)))
    await ready()

    act(() => drop(['/tmp/talk.m4a']))
    expect(tauri.transcribeFile).toHaveBeenCalledWith('/tmp/talk.m4a', {
      speakerLabel: null,
      modelId: null,
      languages: [],
    })
    act(() => report({ stage: 'text', done: 3, total: 12 }))
    expect(screen.getByText('meeting.working 3 12')).toBeTruthy()
    expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('25')

    await act(async () => finish('first line\nsecond line'))
    expect(screen.getByLabelText('meeting.result').textContent).toBe('first line\nsecond line')
  })

  it('names a meeting after what was said in it, unless the user named it', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('we went through the budget')
    vi.mocked(tauri.meetingTitle).mockResolvedValue('Budget review')
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('we went through the budget')
    await waitFor(() =>
      expect((screen.getByLabelText('meeting.titlePlaceholder') as HTMLInputElement).value).toBe(
        'Budget review',
      ),
    )
    expect(tauri.meetingTitle).toHaveBeenCalledWith('we went through the budget')
    expect(vi.mocked(tauri.meetingSave).mock.calls.slice(-1)[0]?.[0].title).toBe('Budget review')
  })

  it('shows the text of a file as far as it is written out', async () => {
    let finish: (text: string) => void = () => {}
    vi.mocked(tauri.transcribeFile).mockReturnValue(new Promise((resolve) => (finish = resolve)))
    await ready()
    act(() => drop(['/tmp/talk.m4a']))
    await waitFor(() => expect(tauri.onFileTranscribeWritten).toHaveBeenCalled())
    act(() => write('the first minute'))
    expect(screen.getByText('the first minute')).toBeTruthy()
    await act(async () => finish('the first minute and the rest'))
    await screen.findByText('the first minute and the rest')
    expect(screen.queryByText('the first minute')).toBeNull()
  })

  it('transcribes a file picked with the button', async () => {
    vi.mocked(tauri.pickMediaFile).mockResolvedValue('/tmp/picked.mp4')
    vi.mocked(tauri.transcribeFile).mockResolvedValue('picked text')
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.chooseFile' }))
    await screen.findByText('picked text')
    expect(vi.mocked(tauri.transcribeFile).mock.calls[0][0]).toBe('/tmp/picked.mp4')
  })

  it('names the speakers without being asked once the models are there', async () => {
    installed(true)
    vi.mocked(tauri.transcribeFile).mockResolvedValue('text')
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.options' }))
    await waitFor(() =>
      expect(
        screen.getByRole('switch', { name: 'meeting.speakers' }).getAttribute('aria-checked'),
      ).toBe('true'),
    )
    act(() => drop(['/tmp/a.mp3']))
    expect(tauri.transcribeFile).toHaveBeenCalledWith('/tmp/a.mp3', {
      speakerLabel: 'meeting.speakerLabel',
      modelId: null,
      languages: [],
    })
    act(() => report({ stage: 'speakers', done: 0, total: 0 }))
    expect(screen.getByText('meeting.findingSpeakers')).toBeTruthy()
  })

  it('leaves speakers alone once the user has switched that off', async () => {
    installed(true)
    window.localStorage.setItem('meeting.speakers', '0')
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    expect(tauri.meetingRecordStart).toHaveBeenCalledWith('', false, false, null, [])
  })

  it('records in the languages the user names, three at most, and remembers them', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.options' }))
    for (const code of ['en', 'zh', 'ja']) {
      fireEvent.change(screen.getByLabelText('meeting.addLanguage'), { target: { value: code } })
    }
    expect(screen.queryByLabelText('meeting.addLanguage')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.removeLanguage 日本語' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    expect(tauri.meetingRecordStart).toHaveBeenCalledWith('', false, false, null, ['en', 'zh'])
    expect(window.localStorage.getItem('meeting.languages')).toBe('en,zh')
  })

  it('takes another language while the meeting is being recorded', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 9, volume: 0, waiting: 0 })
    vi.mocked(tauri.meetingLanguages).mockResolvedValue()
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    await screen.findByRole('button', { name: 'meeting.stop' })
    fireEvent.change(screen.getByLabelText('meeting.addLanguage'), { target: { value: 'en' } })
    expect(tauri.meetingLanguages).toHaveBeenCalledWith(['en'])
  })

  it('takes corrections while the meeting is still being recorded', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 9, volume: 0, waiting: 0 })
    vi.mocked(tauri.meetingRecordStop).mockResolvedValue(null)
    vi.mocked(tauri.meetingLearn).mockResolvedValue(['Tauri'])
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    await screen.findByRole('button', { name: 'meeting.stop' })
    act(() => hear({ speaker: 1, start_ms: 0, text: 'we use Tori' }))
    const words = screen.getByText('we use Tori')
    fireEvent.focus(words)
    // What the same speaker says meanwhile does not move the words being changed.
    act(() => hear({ speaker: 1, start_ms: 5_000, text: 'for the app' }))
    expect(words.textContent).toBe('we use Tori')
    words.textContent = 'we use Tauri'
    fireEvent.blur(words)
    expect(tauri.meetingLearn).toHaveBeenCalledWith('we use Tori', 'we use Tauri')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.stop' }))
    await waitFor(() => expect(tauri.meetingSave).toHaveBeenCalled())
    expect(vi.mocked(tauri.meetingSave).mock.calls[0][0].transcript).toBe(
      'meeting.speakerLabel 1  00:00\nwe use Tauri\n\nmeeting.speakerLabel 1  00:05\nfor the app',
    )
  })

  it('pauses the recording and goes on with it', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 9, volume: 0, waiting: 0 })
    vi.mocked(tauri.meetingRecordPause).mockResolvedValue(true)
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    await screen.findByRole('button', { name: 'meeting.stop' })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.pauseRecording' }))
    expect(tauri.meetingRecordPause).toHaveBeenLastCalledWith(true)
    fireEvent.click(screen.getByRole('button', { name: 'meeting.resume' }))
    expect(tauri.meetingRecordPause).toHaveBeenLastCalledWith(false)
    expect(screen.getByRole('button', { name: 'meeting.pauseRecording' })).toBeTruthy()
  })

  it('marks a note taken during the meeting with its moment, to find it in the sound', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 65, volume: 0, waiting: 0 })
    vi.mocked(tauri.meetingRecordStop).mockResolvedValue('/data/meeting-recording.wav')
    const play = vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue()
    const { container } = render(<MeetingCard />)
    fireEvent.click(await screen.findByRole('button', { name: 'meeting.record' }))
    await screen.findByText('01:05')
    act(() => hear({ speaker: null, start_ms: 0, text: 'what was said' }))
    fireEvent.change(screen.getByLabelText('meeting.notes'), { target: { value: 'budget' } })
    expect((screen.getByLabelText('meeting.notes') as HTMLTextAreaElement).value).toBe(
      '[01:05] budget',
    )
    fireEvent.click(screen.getByRole('button', { name: 'meeting.stop' }))
    fireEvent.click(await screen.findByRole('button', { name: /budget/ }))
    expect(container.querySelector('audio')?.currentTime).toBe(65)
    play.mockRestore()
  })

  it('takes corrections to the text of a meeting that has ended', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('meeting.speakerLabel 1  00:00\nwe use Tori')
    vi.mocked(tauri.meetingLearn).mockResolvedValue([])
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    const words = await screen.findByText('we use Tori')
    words.textContent = 'we use Tauri'
    fireEvent.blur(words)
    expect(tauri.meetingLearn).toHaveBeenCalledWith('we use Tori', 'we use Tauri')
    await waitFor(() =>
      expect(vi.mocked(tauri.meetingSave).mock.calls.slice(-1)[0]?.[0].transcript).toBe(
        'meeting.speakerLabel 1  00:00\nwe use Tauri',
      ),
    )
  })

  it('tidies up the whole text when asked, and puts it back', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue(
      'meeting.speakerLabel 1  00:00\nwe use tori\n\nmeeting.speakerLabel 2  00:04\nok',
    )
    vi.mocked(tauri.meetingPolish).mockResolvedValue(['We use Tauri.', 'OK.'])
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('we use tori')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.polish' }))
    expect(tauri.meetingPolish).toHaveBeenCalledWith(['we use tori', 'ok'])
    await screen.findByText('We use Tauri.')
    expect(screen.getByLabelText('meeting.result').textContent).toBe(
      'meeting.speakerLabel 1  00:00\nWe use Tauri.\n\nmeeting.speakerLabel 2  00:04\nOK.',
    )
    fireEvent.click(screen.getByRole('button', { name: 'meeting.unpolish' }))
    await screen.findByText('we use tori')
  })

  it('says so when the text could not be tidied up', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('hello')
    vi.mocked(tauri.meetingPolish).mockRejectedValue('llm_not_configured')
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('hello')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.polish' }))
    await screen.findByText('meeting.error.llm_not_configured')
    expect(screen.getByText('hello')).toBeTruthy()
  })

  it('tidies up a recording as it ends when that is switched on', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 9, volume: 0, waiting: 0 })
    vi.mocked(tauri.meetingRecordStop).mockResolvedValue(null)
    vi.mocked(tauri.meetingPolish).mockResolvedValue(['What was said.'])
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.options' }))
    fireEvent.click(screen.getByRole('switch', { name: 'meeting.polishAfter' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    await screen.findByRole('button', { name: 'meeting.stop' })
    act(() => hear({ speaker: null, start_ms: 0, text: 'what was said' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.stop' }))
    await screen.findByText('What was said.')
    expect(tauri.meetingPolish).toHaveBeenCalledWith(['what was said'])
    expect(window.localStorage.getItem('meeting.polishAfter')).toBe('1')
  })

  it('downloads the speaker models the first time speakers are switched on', async () => {
    let finish: () => void = () => {}
    vi.mocked(tauri.installSpeakerModels).mockReturnValue(
      new Promise((resolve) => (finish = resolve)),
    )
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.options' }))
    const toggle = screen.getByRole('switch', { name: 'meeting.speakers' })
    fireEvent.click(toggle)
    expect(tauri.installSpeakerModels).toHaveBeenCalled()
    expect(toggle.getAttribute('aria-checked')).toBe('false')
    expect(screen.getByText(/meeting.downloadingSpeakers/)).toBeTruthy()
    await act(async () => finish())
    expect(toggle.getAttribute('aria-checked')).toBe('true')
  })

  it('shows what is said while the meeting is recorded, by speaker', async () => {
    installed(true)
    window.localStorage.setItem('meeting.speakers', '1')
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 75, volume: 0.2, waiting: 0 })
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.options' }))
    await waitFor(() =>
      expect(
        screen.getByRole('switch', { name: 'meeting.speakers' }).getAttribute('aria-checked'),
      ).toBe('true'),
    )
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    await screen.findByText('01:15')
    expect(tauri.meetingRecordStart).toHaveBeenCalledWith('', true, false, null, [])
    expect(screen.getByRole('log').textContent).toBe('meeting.listening')
    act(() => hear({ speaker: 1, start_ms: 0, text: 'good morning' }))
    act(() => hear({ speaker: 2, start_ms: 31_000, text: 'morning' }))
    const live = screen.getByRole('log').textContent
    expect(live).toContain('meeting.speakerLabel 1  00:00good morning')
    expect(live).toContain('meeting.speakerLabel 2  00:31morning')
  })

  it('keeps the text and the audio when the recording is stopped', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 40, volume: 0, waiting: 8 })
    vi.mocked(tauri.meetingRecordStop).mockResolvedValue('/data/meeting-recording.wav')
    vi.mocked(tauri.transcribeFile).mockResolvedValue('the whole meeting again')
    vi.mocked(tauri.saveMeetingAudio).mockResolvedValue(true)
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    await screen.findByRole('button', { name: 'meeting.stop' })
    act(() => hear({ speaker: null, start_ms: 0, text: 'what was said' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.stop' }))
    await screen.findByText('what was said')
    expect(tauri.meetingRecordStop).toHaveBeenCalledWith(false)
    // The recording goes with the meeting, and from then on it can be played and saved.
    await screen.findByRole('button', { name: 'meeting.play' })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.more' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'meeting.saveAudio' }))
    expect(tauri.saveMeetingAudio).toHaveBeenCalledWith(
      expect.stringMatching(/^m-\d+$/),
      'meeting.audioFileName',
    )
    fireEvent.click(screen.getByRole('button', { name: 'meeting.more' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'meeting.again' }))
    await screen.findByText('the whole meeting again')
    expect(vi.mocked(tauri.transcribeFile).mock.calls[0][0]).toBe('/data/meetings/m.wav')
  })

  it('goes on recording while another page is open', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 9, volume: 0, waiting: 0 })
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    await screen.findByRole('button', { name: 'meeting.stop' })
    cleanup()
    act(() => hear({ speaker: null, start_ms: 0, text: 'said while away' }))
    render(<MeetingCard />)
    expect(screen.getByRole('log').textContent).toBe('said while away')
    expect(screen.getByRole('button', { name: 'meeting.stop' })).toBeTruthy()
  })

  it('gives a speaker a name everywhere in the text', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue(
      'meeting.speakerLabel 1  00:00\nhello\n\nmeeting.speakerLabel 2  00:04\nhi',
    )
    vi.mocked(tauri.copyText).mockResolvedValue()
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    fireEvent.change(await screen.findByLabelText('meeting.speakerLabel 2'), {
      target: { value: 'Ann' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.copy' }))
    expect(tauri.copyText).toHaveBeenCalledWith(
      'meeting.speakerLabel 1  00:00\nhello\n\nAnn  00:04\nhi',
    )
  })

  it('remembers the voice of a speaker who is given a name', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('meeting.speakerLabel 1  00:00\nhello')
    vi.mocked(tauri.rememberSpeaker).mockResolvedValue(['Ann'])
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    const input = await screen.findByLabelText('meeting.speakerLabel 1')
    fireEvent.change(input, { target: { value: 'Ann' } })
    fireEvent.blur(input)
    expect(tauri.rememberSpeaker).toHaveBeenCalledWith(1, 'Ann')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.all' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.options' }))
    await screen.findByText('meeting.voices 1')
  })

  it('names a speaker whose voice was remembered, unless the user typed a name', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue(
      'meeting.speakerLabel 1  00:00\nhello\n\nmeeting.speakerLabel 2  00:04\nhi',
    )
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    fireEvent.change(await screen.findByLabelText('meeting.speakerLabel 2'), {
      target: { value: 'Typed' },
    })
    act(() => name({ 1: 'Ann', 2: 'Bo' }))
    expect(screen.getByLabelText('meeting.result').textContent).toBe(
      'Ann  00:00\nhello\n\nTyped  00:04\nhi',
    )
    // A name is set in its speaker's colour, apart from the words.
    expect(screen.getByText('Ann').style.color).toBe('var(--speaker-1)')
    expect(screen.getByText('Typed').style.color).toBe('var(--speaker-2)')
  })

  it('learns the names a meeting ends with, but not from a past meeting', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('meeting.speakerLabel 1  00:00\nhello')
    vi.mocked(tauri.speakerConfirm).mockResolvedValue(['Ann'])
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByLabelText('meeting.speakerLabel 1')
    act(() => name({ 1: 'Ann' }))
    fireEvent.blur(screen.getByLabelText('meeting.notes'))
    await waitFor(() => expect(tauri.speakerConfirm).toHaveBeenCalledWith({ 1: 'Ann' }))
    vi.mocked(tauri.speakerConfirm).mockClear()
    act(() => useMeetingStore.setState({ voicesAtHand: false }))
    fireEvent.blur(screen.getByLabelText('meeting.notes'))
    await waitFor(() => expect(tauri.meetingSave).toHaveBeenCalledTimes(3))
    expect(tauri.speakerConfirm).not.toHaveBeenCalled()
  })

  it("records the computer's own sound too when that is switched on", async () => {
    vi.mocked(tauri.meetingRecordStart).mockRejectedValue('system_audio_unsupported')
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.options' }))
    fireEvent.click(screen.getByRole('switch', { name: 'meeting.systemSound' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    expect(tauri.meetingRecordStart).toHaveBeenCalledWith('', false, true, null, [])
    await screen.findByText('meeting.error.system_audio_unsupported')
    expect(window.localStorage.getItem('meeting.systemSound')).toBe('1')
  })

  it('forgets a remembered voice', async () => {
    vi.mocked(tauri.speakerVoices).mockResolvedValue(['Ann', 'Bo'])
    vi.mocked(tauri.forgetSpeaker).mockResolvedValue(['Bo'])
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.options' }))
    await screen.findByText('meeting.voices 2')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.forgetVoice Ann' }))
    await waitFor(() => expect(screen.queryByText('Ann')).toBeNull())
    expect(tauri.forgetSpeaker).toHaveBeenCalledWith('Ann')
  })

  it('keeps a meeting among the past meetings once it has text', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('hello')
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('hello')
    await waitFor(() => expect(tauri.meetingSave).toHaveBeenCalled())
    const saved = vi.mocked(tauri.meetingSave).mock.calls[0][0]
    expect(saved.transcript).toBe('hello')
    expect(saved.id).toMatch(/^m-\d+$/)
  })

  it('opens a past meeting with its notes and minutes, and deletes one', async () => {
    vi.mocked(tauri.meetingsList).mockResolvedValue([
      { id: 'm-1', started_at: 1_700_000_000_000, preview: 'budget talk', has_minutes: true },
    ])
    vi.mocked(tauri.meetingGet).mockResolvedValue({
      id: 'm-1',
      started_at: 1_700_000_000_000,
      transcript: 'meeting.speakerLabel 1  00:00\nthe budget',
      notes: 'my note',
      minutes: 'old minutes',
      names: { 1: 'Ann' },
    })
    vi.mocked(tauri.meetingDelete).mockResolvedValue()
    await ready()
    fireEvent.click(await screen.findByText('budget talk'))
    await waitFor(() =>
      expect(screen.getByLabelText('meeting.result').textContent).toBe('Ann  00:00\nthe budget'),
    )
    expect(screen.getByText('my note')).toBeTruthy()
    expect(screen.getByText('old minutes')).toBeTruthy()
    // Back at the list, the meeting can be deleted.
    fireEvent.click(screen.getByRole('button', { name: 'meeting.all' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.deleteMeeting' }))
    await waitFor(() => expect(tauri.meetingDelete).toHaveBeenCalledWith('m-1'))
  })

  it('hands the notes taken during the meeting to the minutes', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 9, volume: 0, waiting: 0 })
    vi.mocked(tauri.meetingRecordStop).mockResolvedValue(null)
    vi.mocked(tauri.meetingMinutes).mockResolvedValue('the minutes')
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    fireEvent.change(await screen.findByLabelText('meeting.notes'), {
      target: { value: 'cut the budget' },
    })
    act(() => hear({ speaker: null, start_ms: 0, text: 'what was said' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.stop' }))
    fireEvent.click(await screen.findByRole('tab', { name: 'meeting.tabMinutes' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.minutes' }))
    await screen.findByText('the minutes')
    expect(tauri.meetingMinutes).toHaveBeenCalledWith(
      'what was said',
      'meeting.defaultTemplate',
      expect.stringMatching(/^\[00:0\d\] cut the budget$/),
    )
  })

  it('says how much is left to recognize after stopping', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 40, volume: 0, waiting: 8 })
    vi.mocked(tauri.meetingRecordStop).mockReturnValue(new Promise(() => {}))
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    fireEvent.click(await screen.findByRole('button', { name: 'meeting.stop' }))
    await screen.findByText('meeting.finishing 8')
  })

  it('deletes the kept audio when asked', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 5, volume: 0, waiting: 0 })
    vi.mocked(tauri.meetingRecordStop).mockResolvedValue('/data/meeting-recording.wav')
    vi.mocked(tauri.meetingAudioDelete).mockResolvedValue()
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    await screen.findByRole('button', { name: 'meeting.stop' })
    act(() => hear({ speaker: null, start_ms: 0, text: 'said' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.stop' }))
    await screen.findByRole('button', { name: 'meeting.play' })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.more' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'meeting.deleteAudio' }))
    await waitFor(() => expect(screen.queryByRole('button', { name: 'meeting.play' })).toBeNull())
    expect(tauri.meetingAudioDelete).toHaveBeenCalledWith(expect.stringMatching(/^m-\d+$/))
  })

  it('drops a recording that is discarded', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 2, volume: 0, waiting: 0 })
    vi.mocked(tauri.meetingRecordStop).mockResolvedValue(null)
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    fireEvent.click(await screen.findByRole('button', { name: 'meeting.discard' }))
    await screen.findByRole('button', { name: 'meeting.record' })
    expect(tauri.meetingRecordStop).toHaveBeenCalledWith(true)
    expect(screen.queryByLabelText('meeting.result')).toBeNull()
  })

  it('offers minutes once there is text', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('hello')
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    expect(screen.queryByRole('button', { name: 'meeting.minutes' })).toBeNull()
    fireEvent.click(await screen.findByRole('tab', { name: 'meeting.tabMinutes' }))
    expect(screen.getByRole('button', { name: 'meeting.minutes' })).toBeTruthy()
  })

  it('copies and saves the text', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('hello')
    vi.mocked(tauri.copyText).mockResolvedValue()
    vi.mocked(tauri.saveTextFile).mockResolvedValue(true)
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    fireEvent.click(await screen.findByRole('button', { name: 'meeting.copy' }))
    expect(tauri.copyText).toHaveBeenCalledWith('hello')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.save' }))
    expect(vi.mocked(tauri.saveTextFile).mock.calls[0][0]).toBe('hello')
  })

  it('says why a file could not be turned into text', async () => {
    vi.mocked(tauri.transcribeFile).mockRejectedValue('file_not_audio')
    await ready()
    act(() => drop(['/tmp/a.txt']))
    await screen.findByText('meeting.error.file_not_audio')
  })

  it('offers the installed models when there is more than one', async () => {
    const model = (id: string, isInstalled: boolean) => ({
      id,
      size_bytes: 1,
      recommended: false,
      installed: isInstalled,
      installing: false,
      partial_bytes: 0,
    })
    vi.mocked(tauri.listLocalModels).mockResolvedValue([
      model('qwen3-asr-1.7b-q8', true),
      model('qwen3-asr-1.7b-f16', true),
      model('cohere-transcribe-q8', false),
    ])
    vi.mocked(tauri.transcribeFile).mockResolvedValue('text')
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.options' }))
    fireEvent.click(await screen.findByRole('combobox', { name: 'meeting.model' }))
    expect(screen.getAllByRole('option', { name: /Qwen3/ })).toHaveLength(2)
    fireEvent.click(screen.getByRole('option', { name: 'Qwen3 1.7B F16' }))
    act(() => drop(['/tmp/a.mp3']))
    expect(vi.mocked(tauri.transcribeFile).mock.calls[0][1]?.modelId).toBe('qwen3-asr-1.7b-f16')
  })
})

describe('a meeting that is open', () => {
  beforeEach(() => {
    useMeetingStore.setState({ ...initialMeeting })
    vi.mocked(tauri.onFileDropped).mockResolvedValue(() => {})
    vi.mocked(tauri.onFileTranscribeProgress).mockResolvedValue(() => {})
    vi.mocked(tauri.onSpeakerModelsProgress).mockResolvedValue(() => {})
    vi.mocked(tauri.speakerModelsStatus).mockResolvedValue({
      installed: false,
      downloading: false,
      total_bytes: 0,
    })
    vi.mocked(tauri.speakerVoices).mockResolvedValue([])
    vi.mocked(tauri.listLocalModels).mockResolvedValue([])
    vi.mocked(tauri.minutesTemplates).mockResolvedValue([])
    vi.mocked(tauri.meetingSave).mockResolvedValue()
    vi.mocked(tauri.meetingDelete).mockResolvedValue()
    vi.mocked(tauri.audioSource).mockImplementation((path) => path)
    vi.mocked(tauri.meetingsList).mockResolvedValue([
      { id: 'm-1', started_at: 1_700_000_000_000, preview: 'budget talk', has_minutes: false },
    ])
    vi.mocked(tauri.meetingGet).mockResolvedValue({
      id: 'm-1',
      started_at: 1_700_000_000_000,
      transcript: 'meeting.speakerLabel 1  01:05\nthe budget',
      notes: '',
      minutes: '',
      names: {},
      title: 'Budget',
      duration_s: 754,
    })
  })
  afterEach(cleanup)

  it('carries its name and length, and takes a new name', async () => {
    vi.mocked(tauri.meetingAudio).mockResolvedValue(null)
    render(<MeetingCard />)
    fireEvent.click(await screen.findByText('budget talk'))
    const title = await screen.findByLabelText<HTMLInputElement>('meeting.titlePlaceholder')
    expect(title.value).toBe('Budget')
    expect(screen.getByText('12:34')).toBeTruthy()
    fireEvent.change(title, { target: { value: 'Budget review' } })
    fireEvent.blur(title)
    await waitFor(() =>
      expect(vi.mocked(tauri.meetingSave).mock.calls.slice(-1)[0]?.[0].title).toBe('Budget review'),
    )
  })

  it('plays from the moment a time in the text names, when its sound is kept', async () => {
    vi.mocked(tauri.meetingAudio).mockResolvedValue('/data/meetings/m-1.wav')
    const play = vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue()
    const { container } = render(<MeetingCard />)
    fireEvent.click(await screen.findByText('budget talk'))
    fireEvent.click(await screen.findByRole('button', { name: '01:05' }))
    expect(container.querySelector('audio')?.currentTime).toBe(65)
    expect(play).toHaveBeenCalled()
    play.mockRestore()
  })

  it('moves to the moment of the words that are clicked, without starting to play', async () => {
    vi.mocked(tauri.meetingAudio).mockResolvedValue('/data/meetings/m-1.wav')
    const play = vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue()
    const { container } = render(<MeetingCard />)
    fireEvent.click(await screen.findByText('budget talk'))
    await screen.findByRole('button', { name: 'meeting.play' })
    fireEvent.click(screen.getByText('the budget'))
    expect(container.querySelector('audio')?.currentTime).toBe(65)
    expect(play).not.toHaveBeenCalled()
    play.mockRestore()
  })

  it('greys the words the sound has not reached yet', async () => {
    vi.mocked(tauri.meetingAudio).mockResolvedValue('/data/meetings/m-1.wav')
    vi.mocked(tauri.meetingGet).mockResolvedValue({
      id: 'm-1',
      started_at: 1_700_000_000_000,
      transcript:
        'meeting.speakerLabel 1  00:00\nhello\n\nmeeting.speakerLabel 2  01:05\nthe budget',
      notes: '',
      minutes: '',
      names: {},
    })
    const { container } = render(<MeetingCard />)
    fireEvent.click(await screen.findByText('budget talk'))
    await screen.findByRole('button', { name: 'meeting.play' })
    const later = () => screen.getByText('the budget').className
    expect(later()).not.toContain('text-text-tertiary')
    const audio = container.querySelector('audio') as HTMLAudioElement
    audio.currentTime = 10
    fireEvent.timeUpdate(audio)
    expect(later()).toContain('text-text-tertiary')
    expect(screen.getByText('hello').className).not.toContain('text-text-tertiary')
    audio.currentTime = 70
    fireEvent.timeUpdate(audio)
    expect(later()).not.toContain('text-text-tertiary')
  })

  it('is deleted from the menu, which leads back to the list', async () => {
    vi.mocked(tauri.meetingAudio).mockResolvedValue(null)
    render(<MeetingCard />)
    fireEvent.click(await screen.findByText('budget talk'))
    fireEvent.click(await screen.findByRole('button', { name: 'meeting.more' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'meeting.deleteThis' }))
    expect(tauri.meetingDelete).toHaveBeenCalledWith('m-1')
    await screen.findByRole('button', { name: 'meeting.record' })
  })
})
