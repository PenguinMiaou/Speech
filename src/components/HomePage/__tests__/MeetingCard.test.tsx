import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { MeetingCard } from '../MeetingCard'
import * as tauri from '../../../lib/tauri'

vi.mock('../../../lib/tauri')
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, unknown>) =>
      values ? `${key} ${Object.values(values).join(' ')}` : key,
  }),
}))

let drop: (paths: string[]) => void = () => {}
let report: (progress: tauri.FileTranscribeProgress) => void = () => {}

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
    vi.mocked(tauri.onFileDropped).mockImplementation(async (handler) => {
      drop = handler
      return () => {}
    })
    vi.mocked(tauri.onFileTranscribeProgress).mockImplementation(async (handler) => {
      report = handler
      return () => {}
    })
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
    })
    act(() => report({ stage: 'text', done: 3, total: 12 }))
    expect(screen.getByText('meeting.working 3 12')).toBeTruthy()
    expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('25')

    await act(async () => finish('first line\nsecond line'))
    expect((screen.getByLabelText('meeting.result') as HTMLTextAreaElement).value).toBe(
      'first line\nsecond line',
    )
  })

  it('transcribes a file picked with the button', async () => {
    vi.mocked(tauri.pickMediaFile).mockResolvedValue('/tmp/picked.mp4')
    vi.mocked(tauri.transcribeFile).mockResolvedValue('picked text')
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.chooseFile' }))
    await screen.findByDisplayValue('picked text')
    expect(vi.mocked(tauri.transcribeFile).mock.calls[0][0]).toBe('/tmp/picked.mp4')
  })

  it('names the speakers when that is switched on and the models are there', async () => {
    installed(true)
    vi.mocked(tauri.transcribeFile).mockResolvedValue('text')
    await ready()
    fireEvent.click(screen.getByRole('switch', { name: 'meeting.speakers' }))
    act(() => drop(['/tmp/a.mp3']))
    expect(tauri.transcribeFile).toHaveBeenCalledWith('/tmp/a.mp3', {
      speakerLabel: 'meeting.speakerLabel',
      modelId: null,
    })
    act(() => report({ stage: 'speakers', done: 0, total: 0 }))
    expect(screen.getByText('meeting.findingSpeakers')).toBeTruthy()
  })

  it('downloads the speaker models the first time speakers are switched on', async () => {
    let finish: () => void = () => {}
    vi.mocked(tauri.installSpeakerModels).mockReturnValue(
      new Promise((resolve) => (finish = resolve)),
    )
    await ready()
    const toggle = screen.getByRole('switch', { name: 'meeting.speakers' })
    fireEvent.click(toggle)
    expect(tauri.installSpeakerModels).toHaveBeenCalled()
    expect(toggle.getAttribute('aria-checked')).toBe('false')
    expect(screen.getByText(/meeting.downloadingSpeakers/)).toBeTruthy()
    await act(async () => finish())
    expect(toggle.getAttribute('aria-checked')).toBe('true')
  })

  it('records a meeting and writes it out when stopped', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 75, volume: 0.2 })
    vi.mocked(tauri.meetingRecordStop).mockResolvedValue('what was said')
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    await screen.findByText('01:15')
    fireEvent.click(screen.getByRole('button', { name: 'meeting.stop' }))
    await screen.findByDisplayValue('what was said')
    expect(tauri.meetingRecordStop).toHaveBeenCalledWith({
      discard: false,
      speakerLabel: null,
      modelId: null,
    })
  })

  it('drops a recording that is discarded', async () => {
    vi.mocked(tauri.meetingRecordStart).mockResolvedValue()
    vi.mocked(tauri.meetingRecordLevel).mockResolvedValue({ seconds: 2, volume: 0 })
    vi.mocked(tauri.meetingRecordStop).mockResolvedValue('')
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.record' }))
    fireEvent.click(await screen.findByRole('button', { name: 'meeting.discard' }))
    await screen.findByRole('button', { name: 'meeting.record' })
    expect(vi.mocked(tauri.meetingRecordStop).mock.calls[0][0].discard).toBe(true)
    expect(screen.queryByLabelText('meeting.result')).toBeNull()
  })

  it('copies and saves the text', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('hello')
    vi.mocked(tauri.copyText).mockResolvedValue()
    vi.mocked(tauri.saveTextFile).mockResolvedValue(true)
    await ready()
    act(() => drop(['/tmp/a.mp3']))
    fireEvent.click(await screen.findByText('meeting.copy'))
    expect(tauri.copyText).toHaveBeenCalledWith('hello')
    fireEvent.click(screen.getByText('meeting.save'))
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
    fireEvent.click(await screen.findByRole('combobox', { name: 'meeting.model' }))
    expect(screen.getAllByRole('option')).toHaveLength(2)
    fireEvent.click(screen.getByRole('option', { name: 'Qwen3 1.7B F16' }))
    act(() => drop(['/tmp/a.mp3']))
    expect(vi.mocked(tauri.transcribeFile).mock.calls[0][1]?.modelId).toBe('qwen3-asr-1.7b-f16')
  })
})
