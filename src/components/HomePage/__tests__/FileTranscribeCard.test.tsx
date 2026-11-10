import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { FileTranscribeCard } from '../FileTranscribeCard'
import * as tauri from '../../../lib/tauri'

vi.mock('../../../lib/tauri')
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, unknown>) =>
      values ? `${key} ${Object.values(values).join(' ')}` : key,
  }),
}))

let drop: (paths: string[]) => void = () => {}
let report: (progress: { done: number; total: number }) => void = () => {}

describe('FileTranscribeCard', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(tauri.onFileDropped).mockImplementation(async (handler) => {
      drop = handler
      return () => {}
    })
    vi.mocked(tauri.onFileTranscribeProgress).mockImplementation(async (handler) => {
      report = handler
      return () => {}
    })
  })
  afterEach(cleanup)

  it('invites a file to be dropped', () => {
    render(<FileTranscribeCard />)
    expect(screen.getByText('fileTranscribe.drop')).toBeTruthy()
  })

  it('shows progress, then the text of a dropped file', async () => {
    let finish: (text: string) => void = () => {}
    vi.mocked(tauri.transcribeFile).mockReturnValue(new Promise((resolve) => (finish = resolve)))
    render(<FileTranscribeCard />)
    await waitFor(() => expect(tauri.onFileDropped).toHaveBeenCalled())

    act(() => drop(['/tmp/talk.m4a']))
    expect(tauri.transcribeFile).toHaveBeenCalledWith('/tmp/talk.m4a')
    act(() => report({ done: 3, total: 12 }))
    expect(screen.getByText('fileTranscribe.working 3 12')).toBeTruthy()

    await act(async () => finish('first line\nsecond line'))
    expect((screen.getByLabelText('fileTranscribe.result') as HTMLTextAreaElement).value).toBe(
      'first line\nsecond line',
    )
  })

  it('copies the text', async () => {
    vi.mocked(tauri.transcribeFile).mockResolvedValue('hello')
    vi.mocked(tauri.copyText).mockResolvedValue()
    render(<FileTranscribeCard />)
    await waitFor(() => expect(tauri.onFileDropped).toHaveBeenCalled())
    act(() => drop(['/tmp/a.mp3']))
    fireEvent.click(await screen.findByText('fileTranscribe.copy'))
    expect(tauri.copyText).toHaveBeenCalledWith('hello')
  })

  it('says why a file could not be turned into text', async () => {
    vi.mocked(tauri.transcribeFile).mockRejectedValue('file_not_audio')
    render(<FileTranscribeCard />)
    await waitFor(() => expect(tauri.onFileDropped).toHaveBeenCalled())
    act(() => drop(['/tmp/notes.txt']))
    expect(await screen.findByText('fileTranscribe.error.file_not_audio')).toBeTruthy()
  })

  it('ignores a second file while one is being worked on', async () => {
    vi.mocked(tauri.transcribeFile).mockReturnValue(new Promise(() => {}))
    render(<FileTranscribeCard />)
    await waitFor(() => expect(tauri.onFileDropped).toHaveBeenCalled())
    act(() => drop(['/tmp/a.mp3']))
    act(() => drop(['/tmp/b.mp3']))
    expect(tauri.transcribeFile).toHaveBeenCalledTimes(1)
  })
})
