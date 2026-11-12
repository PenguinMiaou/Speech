import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { MeetingCard } from '../MeetingCard'
import { useAppStore } from '../../../stores/appStore'
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
let tidied: (progress: tauri.MeetingPolishing) => void = () => {}
let ask: (terms: tauri.MeetingTerm[]) => void = () => {}
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
    vi.mocked(tauri.speakerFewer).mockReset()
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
    vi.mocked(tauri.onMeetingPolishing).mockImplementation(async (handler) => {
      tidied = handler
      return () => {}
    })
    vi.mocked(tauri.onMeetingTerms).mockImplementation(async (handler) => {
      ask = handler
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
    expect(screen.getByText('meeting.working 25')).toBeTruthy()
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
    act(() => write('the first minute'))
    expect(screen.getByText('the first minute')).toBeTruthy()
    await act(async () => finish('the first minute and the rest'))
    await screen.findByText('the first minute and the rest')
    expect(screen.queryByText('the first minute')).toBeNull()
  })

  it('shows that the sound goes to the recognizer chosen in the settings when that is away', async () => {
    const config = useAppStore.getState().config
    useAppStore.setState({ config: { ...config, stt_provider: 'groq-whisper' } })
    await ready()
    expect(screen.getByLabelText('meeting.heardAway')).toBeTruthy()
    act(() => useAppStore.setState({ config }))
    expect(screen.queryByLabelText('meeting.heardAway')).toBeNull()
  })

  it('names a failure of the recognizer chosen in the settings', async () => {
    vi.mocked(tauri.transcribeFile).mockReset()
    vi.mocked(tauri.transcribeFile).mockRejectedValue('stt_invalid_key')
    await ready()
    act(() => drop(['/tmp/talk.m4a']))
    await waitFor(() =>
      expect(useMeetingStore.getState().error).toBe('meeting.error.stt_invalid_key'),
    )
  })

  it('keeps what was written of a file that could not be finished, and says it failed', async () => {
    let fail: (reason: string) => void = () => {}
    vi.mocked(tauri.transcribeFile).mockReset()
    vi.mocked(tauri.transcribeFile).mockReturnValue(new Promise((_, reject) => (fail = reject)))
    vi.mocked(tauri.meetingSave).mockClear()
    await ready()
    act(() => drop(['/tmp/one.mp3']))
    act(() => write('the first minute'))
    await act(async () => fail('local speech worker timed out'))
    await waitFor(() =>
      expect(vi.mocked(tauri.meetingSave).mock.lastCall?.[0].transcript).toBe('the first minute'),
    )
    expect(useMeetingStore.getState().error).toBe('meeting.error.failed')
  })

  it('stops writing a file out when asked, and keeps what is written so far', async () => {
    let fail: (reason: string) => void = () => {}
    vi.mocked(tauri.transcribeFile).mockReset()
    vi.mocked(tauri.transcribeFile).mockReturnValue(new Promise((_, reject) => (fail = reject)))
    vi.mocked(tauri.transcribeFileStop).mockResolvedValue()
    vi.mocked(tauri.meetingSave).mockClear()
    await ready()
    act(() => drop(['/tmp/one.mp3', '/tmp/two.mp3']))
    act(() => write('the first minute'))
    fireEvent.click(screen.getByLabelText('meeting.stopFile'))
    expect(tauri.transcribeFileStop).toHaveBeenCalled()
    await act(async () => fail('stopped'))
    await waitFor(() =>
      expect(vi.mocked(tauri.meetingSave).mock.lastCall?.[0].transcript).toBe('the first minute'),
    )
    // The files that waited are let be, and nothing is called a failure.
    expect(tauri.transcribeFile).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('alert')).toBeNull()
    expect(useMeetingStore.getState().error).toBeNull()
  })

  it('turns several dropped files into text one after another, each a meeting of its own', async () => {
    const done: ((text: string) => void)[] = []
    vi.mocked(tauri.transcribeFile).mockReset()
    vi.mocked(tauri.transcribeFile).mockImplementation(
      () => new Promise<string>((resolve) => done.push(resolve)),
    )
    vi.mocked(tauri.meetingSave).mockClear()
    await ready()
    act(() => drop(['/tmp/one.mp3', '/tmp/two.mp3']))
    expect(vi.mocked(tauri.transcribeFile)).toHaveBeenCalledTimes(1)
    expect(screen.getByLabelText('meeting.waiting').textContent).toBe('1')
    await act(async () => done[0]('first text'))
    await waitFor(() => expect(vi.mocked(tauri.transcribeFile)).toHaveBeenCalledTimes(2))
    expect(vi.mocked(tauri.transcribeFile).mock.calls[1][0]).toBe('/tmp/two.mp3')
    await act(async () => done[1]('second text'))
    await screen.findByText('second text')
    const saved = vi.mocked(tauri.meetingSave).mock.calls.map(([meeting]) => meeting.transcript)
    expect(saved).toEqual(expect.arrayContaining(['first text', 'second text']))
  })

  it('goes on with the next file when one cannot be turned into text, and says so', async () => {
    vi.mocked(tauri.transcribeFile).mockReset()
    vi.mocked(tauri.transcribeFile)
      .mockRejectedValueOnce('file_not_audio')
      .mockResolvedValueOnce('second text')
    await ready()
    act(() => drop(['/tmp/one.txt', '/tmp/two.mp3']))
    await screen.findByText('second text')
    expect(screen.getByText('meeting.error.someFailed')).toBeTruthy()
  })

  it('transcribes a file picked with the button', async () => {
    vi.mocked(tauri.pickMediaFile).mockResolvedValue(['/tmp/picked.mp4'])
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
    const state = (step: string) => screen.getByLabelText(step).getAttribute('data-state')
    expect(state('meeting.stepRead')).toBe('done')
    expect(state('meeting.stepSpeakers')).toBe('now')
    expect(state('meeting.stepText')).toBe('next')
  })

  it('still knows how far the file has got after another page was open', async () => {
    vi.mocked(tauri.transcribeFile).mockReturnValue(new Promise(() => {}))
    const { unmount } = render(<MeetingCard />)
    await waitFor(() => expect(tauri.speakerModelsStatus).toHaveBeenCalled())
    act(() => drop(['/tmp/a.mp3']))
    unmount()
    act(() => report({ stage: 'speakers', done: 0, total: 0 }))
    act(() => write('meeting.speakerLabel 1  00:00\nso far'))
    render(<MeetingCard />)
    expect(screen.getByText('meeting.findingSpeakers')).toBeTruthy()
    expect(screen.getByText('so far')).toBeTruthy()
  })

  it('shows how long the file has been worked on', async () => {
    vi.mocked(tauri.transcribeFile).mockReturnValue(new Promise(() => {}))
    await ready()
    vi.useFakeTimers()
    try {
      act(() => drop(['/tmp/a.mp3']))
      expect(screen.getByRole('timer').textContent).toBe('00:00')
      act(() => void vi.advanceTimersByTime(65_000))
      expect(screen.getByRole('timer').textContent).toBe('01:05')
    } finally {
      vi.useRealTimers()
    }
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

  it('settles who spoke when a recording ends', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 9, volume: 0, waiting: 0 })
    vi.mocked(tauri.meetingRecordStop).mockResolvedValue(null)
    vi.mocked(tauri.speakerFewer).mockResolvedValue([[2, 1]])
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    await screen.findByRole('button', { name: 'meeting.stop' })
    act(() => hear({ speaker: 1, start_ms: 0, text: 'hello there' }))
    act(() => hear({ speaker: 2, start_ms: 5_000, text: 'ok' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.stop' }))
    await waitFor(() => expect(tauri.meetingSave).toHaveBeenCalled())
    expect(vi.mocked(tauri.meetingSave).mock.calls[0][0].transcript).toBe(
      'meeting.speakerLabel 1  00:00\nhello there\n\nmeeting.speakerLabel 1  00:05\nok',
    )
    expect(tauri.speakerFewer).toHaveBeenCalledWith({ 1: 10, 2: 2 }, 0)
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
    expect(vi.mocked(tauri.meetingPolish).mock.lastCall?.[0]).toEqual(['we use tori', 'ok'])
    await waitFor(() =>
      expect(useMeetingStore.getState().text).toBe(
        'meeting.speakerLabel 1  00:00\nWe use Tauri.\n\nmeeting.speakerLabel 2  00:04\nOK.',
      ),
    )
    // The tidied text is shown as it now reads; what changed is shown when asked for:
    // the words taken out struck through, the new ones marked. Asked again, it goes.
    expect(document.querySelector('del')).toBeNull()
    const compare = screen.getByRole('button', { name: 'meeting.compare' })
    fireEvent.click(compare)
    expect(compare.getAttribute('aria-pressed')).toBe('true')
    expect([...document.querySelectorAll('del')].map((gone) => gone.textContent)).toContain('tori')
    fireEvent.click(compare)
    expect(document.querySelector('del')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.more' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'meeting.unpolish' }))
    await screen.findByText('we use tori')
  })

  it('shows how far the tidying has got, the words first and then who said them', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('meeting.speakerLabel 1  00:00\nwe use tori')
    vi.mocked(tauri.meetingPolish).mockReturnValue(new Promise(() => {}))
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('we use tori')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.polish' }))
    const now = () =>
      screen.getByRole('progressbar', { name: 'meeting.polish' }).getAttribute('aria-valuenow')
    act(() => tidied({ step: 'words', done: 1, total: 2 }))
    expect(now()).toBe('25')
    act(() => tidied({ step: 'speakers', done: 1, total: 2 }))
    expect(now()).toBe('75')
    // Each step is a node: the one that is over is ticked, the one going on is marked.
    const state = (step: string) => screen.getByLabelText(step).getAttribute('data-state')
    expect(state('meeting.stepWords')).toBe('done')
    expect(state('meeting.stepSpeakers')).toBe('now')
  })

  const two = [
    { id: 'm-1', started_at: 1, preview: 'first words', has_minutes: false },
    { id: 'm-2', started_at: 2, preview: 'other words', has_minutes: true },
  ]
  const record = {
    id: 'm-1',
    started_at: 1,
    transcript: 'first words',
    notes: '',
    minutes: '',
    names: {},
    title: '',
    duration_s: 9,
    times: [],
  }

  it('makes minutes of a meeting in the list in one go, after the template picked', async () => {
    vi.mocked(tauri.meetingsList).mockResolvedValue(two)
    vi.mocked(tauri.meetingGet).mockResolvedValue(record)
    vi.mocked(tauri.meetingPolish).mockResolvedValue(['First words.'])
    vi.mocked(tauri.meetingMinutes).mockResolvedValue('the minutes')
    await ready()
    await screen.findByText('first words')
    fireEvent.click(screen.getAllByRole('button', { name: 'meeting.minutes' })[0])
    const gallery = within(await screen.findByRole('dialog'))
    // Nothing begins before the template is confirmed.
    expect(tauri.meetingGet).not.toHaveBeenCalled()
    // Without kept sound there is nothing to write out anew.
    expect(gallery.queryByRole('checkbox', { name: 'meeting.againFirst' })).toBeNull()
    fireEvent.click(gallery.getByRole('radio', { name: 'meeting.standupTemplateName' }))
    fireEvent.click(gallery.getByRole('button', { name: 'meeting.minutes' }))
    await waitFor(() =>
      expect(tauri.meetingMinutes).toHaveBeenCalledWith(
        'First words.',
        'meeting.standupTemplate',
        '',
      ),
    )
    expect(screen.queryByRole('dialog')).toBeNull()
    expect(tauri.transcribeFile).not.toHaveBeenCalled()
    await waitFor(() => expect(tauri.meetingsList).toHaveBeenCalledTimes(2))
  })

  it('writes the kept sound out anew first where that is left ticked', async () => {
    vi.mocked(tauri.meetingsList).mockResolvedValue(two)
    vi.mocked(tauri.meetingGet).mockResolvedValue(record)
    vi.mocked(tauri.meetingAudio).mockResolvedValue('/data/meetings/m-1.wav')
    vi.mocked(tauri.transcribeFile).mockResolvedValue('heard anew')
    vi.mocked(tauri.meetingPolish).mockResolvedValue(['Heard anew.'])
    vi.mocked(tauri.meetingMinutes).mockResolvedValue('the minutes')
    await ready()
    await screen.findByText('first words')
    fireEvent.click(screen.getAllByRole('button', { name: 'meeting.minutes' })[0])
    const gallery = within(await screen.findByRole('dialog'))
    const again = (await gallery.findByRole('checkbox', {
      name: 'meeting.againFirst',
    })) as HTMLInputElement
    expect(again.checked).toBe(true)
    fireEvent.click(gallery.getByRole('button', { name: 'meeting.minutes' }))
    await waitFor(() => expect(tauri.meetingMinutes).toHaveBeenCalled())
    expect(vi.mocked(tauri.transcribeFile).mock.calls[0][0]).toBe('/data/meetings/m-1.wav')
    expect(vi.mocked(tauri.meetingMinutes).mock.calls[0][0]).toBe('Heard anew.')
  })

  it('shows in the list which step of making minutes a meeting is at', async () => {
    vi.mocked(tauri.meetingsList).mockResolvedValue(two)
    await ready()
    await screen.findByText('first words')
    act(() =>
      useMeetingStore.setState({
        making: {
          id: 'm-1',
          steps: ['text', 'words', 'speakers', 'minutes'],
          at: 1,
          within: 0.5,
          since: Date.now() - 60_000,
        },
      }),
    )
    const bar = screen.getByRole('progressbar', { name: 'meeting.minutes' })
    expect(bar.getAttribute('aria-valuenow')).toBe('38')
    const steps = [...bar.querySelectorAll('[data-state]')]
    expect(steps.map((step) => step.getAttribute('data-state'))).toEqual([
      'done',
      'now',
      'next',
      'next',
    ])
    // While one meeting is at it, no other can be begun.
    for (const button of screen.getAllByRole('button', { name: 'meeting.minutes' }))
      expect((button as HTMLButtonElement).disabled).toBe(true)
  })

  it('opens the templates from the list, to look at, add to or take from', async () => {
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.template' }))
    const gallery = within(await screen.findByRole('dialog'))
    expect(gallery.getByRole('button', { name: 'meeting.newTemplate' })).toBeTruthy()
    expect(gallery.queryByRole('button', { name: 'meeting.minutes' })).toBeNull()
  })

  it('shows in the list how far a meeting that is being tidied has got', async () => {
    vi.mocked(tauri.meetingsList).mockResolvedValue([
      { id: 'm-1', started_at: 1, preview: 'first words', has_minutes: false },
      { id: 'm-2', started_at: 2, preview: 'other words', has_minutes: false },
    ])
    await ready()
    await screen.findByText('first words')
    act(() =>
      useMeetingStore.setState({
        tidying: { id: 'm-1', fraction: 0.4, since: Date.now() - 60_000, steps: 2 },
      }),
    )
    const bars = screen.getAllByRole('progressbar', { name: 'meeting.polish' })
    expect(bars).toHaveLength(1)
    expect(bars[0].getAttribute('aria-valuenow')).toBe('40')
    // A minute for two fifths: a minute and a half to go.
    expect(screen.getByText('01:30')).toBeTruthy()
  })

  it('puts right who said what after the words, keeping every word', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue(
      'meeting.speakerLabel 1  00:00\nit is done so you\n\nmeeting.speakerLabel 2  00:04\nkeep a copy',
    )
    vi.mocked(tauri.meetingPolish).mockResolvedValue(['It is done. So you', 'keep a copy?'])
    vi.mocked(tauri.meetingSpeakersCheck).mockResolvedValue([
      { speaker: 1, text: 'It is done.' },
      { speaker: 2, text: 'So you keep a copy?' },
    ])
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('it is done so you')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.polish' }))
    await waitFor(() =>
      expect(useMeetingStore.getState().text).toBe(
        'meeting.speakerLabel 1  00:00\nIt is done.\n\nmeeting.speakerLabel 2  00:04\nSo you keep a copy?',
      ),
    )
    expect(tauri.meetingSpeakersCheck).toHaveBeenCalledWith([
      { speaker: 1, text: 'It is done. So you' },
      { speaker: 2, text: 'keep a copy?' },
    ])
  })

  it('gives a paragraph to another speaker when its name is chosen anew', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue(
      'meeting.speakerLabel 1  00:00\nhello\n\nmeeting.speakerLabel 2  00:04\nok',
    )
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('hello')
    fireEvent.doubleClick(screen.getByRole('button', { name: 'meeting.speakerLabel 2' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'meeting.speakerLabel 1' }))
    expect(screen.getByLabelText('meeting.result').textContent).toBe(
      'meeting.speakerLabel 1  00:00\nhello\n\n00:04\nok',
    )
  })

  it('takes the surplus speakers for the most alike when told how many people met', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue(
      'meeting.speakerLabel 1  00:00\nhello\n\nmeeting.speakerLabel 2  00:04\nok\n\nmeeting.speakerLabel 3  00:08\nyes',
    )
    vi.mocked(tauri.speakerFewer).mockImplementation((_, most) =>
      Promise.resolve(most === 2 ? [[3, 1]] : []),
    )
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('hello')
    act(() => useMeetingStore.setState({ voicesAtHand: true }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.people 3' }))
    const people = screen.getByRole('spinbutton', { name: 'meeting.people 3' })
    fireEvent.change(people, { target: { value: '2' } })
    fireEvent.keyDown(people, { key: 'Enter' })
    await waitFor(() =>
      expect(screen.getByLabelText('meeting.result').textContent).toBe(
        'meeting.speakerLabel 1  00:00\nhello\n\nmeeting.speakerLabel 2  00:04\nok\n\nmeeting.speakerLabel 1  00:08\nyes',
      ),
    )
    expect(tauri.speakerFewer).toHaveBeenCalledWith({ 1: 5, 2: 2, 3: 3 }, 2)
  })

  it('shows the number of people as plain words where the voices are not at hand', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue(
      'meeting.speakerLabel 1  00:00\nhello\n\nmeeting.speakerLabel 2  00:04\nok',
    )
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('hello')
    act(() => useMeetingStore.setState({ voicesAtHand: false }))
    expect(screen.queryByRole('button', { name: 'meeting.people 2' })).toBeNull()
    expect(screen.getByText('meeting.people 2')).toBeTruthy()
  })

  it('takes a speaker who said next to nothing for the one they sound like', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue(
      'meeting.speakerLabel 1  00:00\nhello\n\nmeeting.speakerLabel 2  00:04\nok\n\nmeeting.speakerLabel 3  00:08\nyes',
    )
    vi.mocked(tauri.speakerFewer).mockResolvedValue([[3, 1]])
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('hello')
    expect(screen.getByLabelText('meeting.result').textContent).toBe(
      'meeting.speakerLabel 1  00:00\nhello\n\nmeeting.speakerLabel 2  00:04\nok\n\nmeeting.speakerLabel 1  00:08\nyes',
    )
    expect(tauri.speakerFewer).toHaveBeenCalledWith({ 1: 5, 2: 2, 3: 3 }, 0)
  })

  it('asks once how the terms it was unsure of are written, and writes them that way', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('we use Tori and tori')
    vi.mocked(tauri.meetingPolish).mockImplementation(async () => {
      ask([{ spelling: 'Tauri', others: ['Tori', 'tori'] }])
      return ['We use Tori and tori.']
    })
    vi.mocked(tauri.addDictionaryEntry).mockResolvedValue()
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('we use Tori and tori')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.polish' }))
    const term = await screen.findByRole('textbox', { name: 'Tori, tori' })
    expect((term as HTMLInputElement).value).toBe('Tauri')
    fireEvent.change(term, { target: { value: 'Tauri 2' } })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.termsApply' }))
    await waitFor(() =>
      expect(vi.mocked(tauri.meetingSave).mock.lastCall?.[0].transcript).toBe(
        'We use Tauri 2 and Tauri 2.',
      ),
    )
    expect(tauri.addDictionaryEntry).toHaveBeenCalledWith('Tauri 2', null)
    expect(screen.queryByRole('textbox', { name: 'Tori, tori' })).toBeNull()
  })

  it('lets the question about the terms be put away', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('we use Tori')
    vi.mocked(tauri.meetingPolish).mockImplementation(async () => {
      ask([{ spelling: 'Tauri', others: ['Tori'] }])
      return ['We use Tori.']
    })
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('we use Tori')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.polish' }))
    await screen.findByRole('textbox', { name: 'Tori' })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.termsSkip' }))
    expect(screen.queryByRole('textbox', { name: 'Tori' })).toBeNull()
    expect(tauri.addDictionaryEntry).not.toHaveBeenCalled()
  })

  it('offers the ways a term was written to pick from, and shows where it was said', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('we use Tori and tori')
    vi.mocked(tauri.meetingPolish).mockImplementation(async () => {
      ask([{ spelling: 'Tauri', others: ['Tori', 'tori'] }])
      return ['We use Tori and tori.']
    })
    vi.mocked(tauri.addDictionaryEntry).mockResolvedValue()
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('we use Tori and tori')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.polish' }))
    const term = await screen.findByRole('textbox', { name: 'Tori, tori' })
    const asked = screen.getByRole('form', { name: 'meeting.terms' })
    expect(asked.textContent).toContain('We use Tori and tori.')
    expect(asked.textContent).toContain('meeting.termsHint')
    fireEvent.click(within(asked).getByRole('button', { name: 'tori' }))
    expect((term as HTMLInputElement).value).toBe('tori')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.termsApply' }))
    await waitFor(() =>
      expect(vi.mocked(tauri.meetingSave).mock.lastCall?.[0].transcript).toBe(
        'We use tori and tori.',
      ),
    )
  })

  it('offers the words of the dictionary written nearly as a term was heard', async () => {
    useAppStore.setState({ dictionary: [{ id: 1, word: 'Torii', pronunciation: null }] })
    vi.mocked(tauri.transcribeFile).mockResolvedValue('we use Tori')
    vi.mocked(tauri.meetingPolish).mockImplementation(async () => {
      ask([{ spelling: 'Tauri', others: ['Tori'] }])
      return ['We use Tori.']
    })
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('we use Tori')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.polish' }))
    const term = await screen.findByRole('textbox', { name: 'Tori' })
    fireEvent.click(screen.getByRole('button', { name: 'Torii' }))
    expect((term as HTMLInputElement).value).toBe('Torii')
    useAppStore.setState({ dictionary: [] })
  })

  it('brings the question about the terms back after it was put away, and keeps it with the meeting', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('we use Tori')
    vi.mocked(tauri.meetingPolish).mockImplementation(async () => {
      ask([{ spelling: 'Tauri', others: ['Tori'] }])
      return ['We use Tori.']
    })
    vi.mocked(tauri.addDictionaryEntry).mockResolvedValue()
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('we use Tori')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.polish' }))
    await screen.findByRole('textbox', { name: 'Tori' })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.termsSkip' }))
    expect(screen.queryByRole('textbox', { name: 'Tori' })).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.terms' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.termsApply' }))
    await waitFor(() =>
      expect(vi.mocked(tauri.meetingSave).mock.lastCall?.[0].terms).toEqual([
        { spelling: 'Tauri', others: ['Tori'] },
      ]),
    )
    // Settled once, it can still be asked about again, with the way it was written before.
    fireEvent.click(screen.getByRole('button', { name: 'meeting.terms' }))
    expect((screen.getByRole('textbox', { name: 'Tori' }) as HTMLInputElement).value).toBe('Tauri')
  })

  it('copies and saves the words without the marks around what is strong', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('we **ship Friday**')
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('ship Friday')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.copy' }))
    expect(tauri.copyText).toHaveBeenCalledWith('we ship Friday')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.save' }))
    expect(tauri.saveTextFile).toHaveBeenCalledWith(
      'we **ship Friday**',
      'meeting.fileName',
      undefined,
      'we ship Friday',
    )
  })

  it('copies and saves a speaker’s name once while they go on', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue(
      'meeting.speakerLabel 1  00:00\nhello\n\nmeeting.speakerLabel 1  00:04\nthere\n\nmeeting.speakerLabel 2  00:08\nok',
    )
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('hello')
    const joined =
      'meeting.speakerLabel 1  00:00\nhello\nthere\n\nmeeting.speakerLabel 2  00:08\nok'
    fireEvent.click(screen.getByRole('button', { name: 'meeting.copy' }))
    expect(tauri.copyText).toHaveBeenCalledWith(joined)
    fireEvent.click(screen.getByRole('button', { name: 'meeting.save' }))
    expect(vi.mocked(tauri.saveTextFile).mock.lastCall?.[0]).toBe(joined)
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
    await waitFor(() => expect(useMeetingStore.getState().text).toContain('What was said.'))
    expect(vi.mocked(tauri.meetingPolish).mock.lastCall?.[0]).toEqual(['what was said'])
    expect(window.localStorage.getItem('meeting.polishAfter')).toBe('1')
  })

  it('downloads the model that times words the first time exact times are switched on', async () => {
    let finish: () => void = () => {}
    vi.mocked(tauri.wordTimesStatus).mockResolvedValue({
      installed: false,
      downloading: false,
      total_bytes: 239_549_735,
    })
    vi.mocked(tauri.onWordTimesProgress).mockResolvedValue(() => {})
    vi.mocked(tauri.installWordTimes).mockReturnValue(new Promise((resolve) => (finish = resolve)))
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.options' }))
    const toggle = screen.getByRole('switch', { name: 'meeting.wordTimes' })
    expect(toggle.getAttribute('aria-checked')).toBe('false')
    fireEvent.click(toggle)
    expect(tauri.installWordTimes).toHaveBeenCalled()
    expect(screen.getByText(/meeting.downloadingModel/)).toBeTruthy()
    await act(async () => finish())
    expect(toggle.getAttribute('aria-checked')).toBe('true')
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
    // It is the same meeting written out anew: one record, and its sound stays with it.
    await waitFor(() =>
      expect(vi.mocked(tauri.meetingSave).mock.lastCall?.[0].transcript).toBe(
        'the whole meeting again',
      ),
    )
    const saved = vi.mocked(tauri.meetingSave).mock.calls.map(([meeting]) => meeting.id)
    expect(new Set(saved).size).toBe(1)
    expect(screen.getByRole('button', { name: 'meeting.play' })).toBeTruthy()
  })

  it('leaves a meeting as it was when writing it out anew fails', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 40, volume: 0, waiting: 8 })
    vi.mocked(tauri.meetingRecordStop).mockResolvedValue('/data/meeting-recording.wav')
    vi.mocked(tauri.transcribeFile).mockRejectedValue('busy')
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    await screen.findByRole('button', { name: 'meeting.stop' })
    act(() => hear({ speaker: null, start_ms: 0, text: 'what was said' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.stop' }))
    await screen.findByRole('button', { name: 'meeting.play' })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.more' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'meeting.again' }))
    await screen.findByText('meeting.error.busy')
    expect(screen.getByText('what was said')).toBeTruthy()
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
    expect(screen.getByRole('log').textContent).toBe('00:00said while away')
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

  it('finds past meetings by what was said in them', async () => {
    vi.mocked(tauri.meetingsList).mockResolvedValue([
      { id: 'm-1', started_at: 1_700_000_000_000, preview: 'budget talk', has_minutes: false },
      { id: 'm-2', started_at: 1_700_000_100_000, preview: 'hiring plan', has_minutes: false },
    ])
    vi.mocked(tauri.meetingsSearch).mockResolvedValue([
      {
        id: 'm-2',
        started_at: 1_700_000_100_000,
        title: 'Weekly',
        duration_s: 60,
        found: '…we need two more engineers…',
      },
    ])
    await ready()
    expect(await screen.findByText('budget talk')).toBeTruthy()

    fireEvent.change(screen.getByLabelText('meeting.search'), { target: { value: 'engineers' } })

    expect(await screen.findByText('…we need two more engineers…')).toBeTruthy()
    expect(tauri.meetingsSearch).toHaveBeenCalledWith('engineers')
    expect(screen.queryByText('budget talk')).toBeNull()

    fireEvent.change(screen.getByLabelText('meeting.search'), { target: { value: '' } })
    expect(await screen.findByText('budget talk')).toBeTruthy()
  })

  it('asks a question of the past meetings together', async () => {
    vi.mocked(tauri.meetingsList).mockResolvedValue([
      { id: 'm-1', started_at: 1_700_000_000_000, preview: 'budget talk', has_minutes: false },
    ])
    vi.mocked(tauri.meetingsSearch).mockResolvedValue([])
    vi.mocked(tauri.meetingsAsk).mockResolvedValue('Ann, in Weekly: two more engineers')
    await ready()
    await screen.findByText('budget talk')
    expect(
      (screen.getByRole('button', { name: 'meeting.askAll' }) as HTMLButtonElement).disabled,
    ).toBe(true)

    fireEvent.change(screen.getByLabelText('meeting.search'), { target: { value: ' who hires? ' } })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.askAll' }))

    expect(await screen.findByText('Ann, in Weekly: two more engineers')).toBeTruthy()
    expect(tauri.meetingsAsk).toHaveBeenCalledWith('who hires?')
  })

  it('keeps what a meeting said before it was tidied, so that a past meeting can still be compared and put back', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('we use tori')
    vi.mocked(tauri.meetingPolish).mockResolvedValue(['We use Tauri.'])
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    await screen.findByText('we use tori')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.polish' }))
    await waitFor(() =>
      expect(vi.mocked(tauri.meetingSave).mock.lastCall?.[0].unpolished?.text).toBe('we use tori'),
    )
    cleanup()
    useMeetingStore.setState({ id: null, text: null, unpolished: null })
    vi.mocked(tauri.meetingsList).mockResolvedValue([
      { id: 'm-1', started_at: 1_700_000_000_000, preview: 'We use Tauri.', has_minutes: false },
    ])
    vi.mocked(tauri.meetingGet).mockResolvedValue({
      id: 'm-1',
      started_at: 1_700_000_000_000,
      transcript: 'We use Tauri.',
      notes: '',
      minutes: '',
      names: {},
      unpolished: { text: 'we use tori' },
    })
    await ready()
    fireEvent.click(await screen.findByText('We use Tauri.'))
    fireEvent.click(await screen.findByRole('button', { name: 'meeting.compare' }))
    expect([...document.querySelectorAll('del')].map((gone) => gone.textContent)).toContain('tori')
  })

  describe('folders', () => {
    const kept = [
      { id: 'm-1', started_at: 2, preview: 'budget talk', has_minutes: false, folder: 'Acme' },
      { id: 'm-2', started_at: 1, preview: 'weekly sync', has_minutes: false, folder: '' },
    ]

    it('shows the meetings of the folder that is picked, and all of them otherwise', async () => {
      vi.mocked(tauri.meetingsList).mockResolvedValue(kept)
      await ready()
      await screen.findByText('weekly sync')
      fireEvent.click(screen.getByRole('button', { name: 'Acme' }))
      expect(screen.getByText('budget talk')).toBeTruthy()
      expect(screen.queryByText('weekly sync')).toBeNull()
      fireEvent.click(screen.getByRole('button', { name: 'meeting.allFolders' }))
      expect(screen.getByText('weekly sync')).toBeTruthy()
    })

    it('puts a meeting in a folder that is there or in a new one, and takes it out again', async () => {
      vi.mocked(tauri.meetingsList).mockResolvedValue(kept)
      vi.mocked(tauri.meetingFolder).mockResolvedValue()
      await ready()
      await screen.findByText('weekly sync')
      fireEvent.click(screen.getAllByRole('button', { name: 'meeting.folder' })[1])
      fireEvent.click(screen.getByRole('menuitem', { name: 'Acme' }))
      await waitFor(() => expect(tauri.meetingFolder).toHaveBeenLastCalledWith('m-2', 'Acme'))
      fireEvent.click(screen.getAllByRole('button', { name: 'meeting.folder' })[1])
      const name = screen.getByRole('textbox', { name: 'meeting.newFolder' })
      fireEvent.change(name, { target: { value: 'Beta' } })
      fireEvent.keyDown(name, { key: 'Enter' })
      await waitFor(() => expect(tauri.meetingFolder).toHaveBeenLastCalledWith('m-2', 'Beta'))
      fireEvent.click(screen.getAllByRole('button', { name: 'meeting.folder' })[0])
      fireEvent.click(screen.getByRole('menuitem', { name: 'meeting.noFolder' }))
      await waitFor(() => expect(tauri.meetingFolder).toHaveBeenLastCalledWith('m-1', ''))
    })

    it('keeps a new meeting in the folder that is picked', async () => {
      vi.mocked(tauri.meetingsList).mockResolvedValue(kept)
      vi.mocked(tauri.transcribeFile).mockResolvedValue('new words')
      await ready()
      await screen.findByText('weekly sync')
      fireEvent.click(screen.getByRole('button', { name: 'Acme' }))
      act(() => drop(['/tmp/a.mp3']))
      await waitFor(() =>
        expect(vi.mocked(tauri.meetingSave).mock.lastCall?.[0].folder).toBe('Acme'),
      )
    })
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
      '00:00\nwhat was said',
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

  it('offers a text with times as subtitles as well', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue(
      'meeting.speakerLabel 1  00:10\nhello\n\nmeeting.speakerLabel 2  00:20\nhi',
    )
    vi.mocked(tauri.saveTextFile).mockResolvedValue(true)
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    fireEvent.click(await screen.findByRole('button', { name: 'meeting.save' }))
    const others = vi.mocked(tauri.saveTextFile).mock.lastCall?.[2]
    expect(others?.srt()).toContain('00:00:10,000 --> 00:00:20,000')
    expect(others?.vtt().startsWith('WEBVTT')).toBe(true)
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
