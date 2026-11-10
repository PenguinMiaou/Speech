import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { VoiceIsolationSection } from '../VoiceIsolationSection'
import * as tauri from '../../../lib/tauri'
import { useAppStore } from '../../../stores/appStore'

vi.mock('../../../lib/tauri')
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, string | number>) =>
      values?.size ? `${key}(${values.size})` : key,
  }),
}))

const STATUS: tauri.VoiceIsolationStatus = {
  modelInstalled: true,
  downloading: false,
  hasSample: true,
  enrollmentPending: false,
  enrollmentError: null,
  modelBytes: 79_084_924,
}

function status(change: Partial<tauri.VoiceIsolationStatus>) {
  vi.mocked(tauri.voiceIsolationStatus).mockResolvedValue({ ...STATUS, ...change })
}

function switchedOn(voice_isolation_enabled: boolean) {
  useAppStore.setState((state) => ({ config: { ...state.config, voice_isolation_enabled } }))
}

describe('VoiceIsolationSection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    status({})
    vi.mocked(tauri.voiceIsolationInstall).mockResolvedValue()
    vi.mocked(tauri.voiceIsolationEnrollNext).mockResolvedValue()
    vi.mocked(tauri.voiceIsolationDeleteSample).mockResolvedValue()
    vi.mocked(tauri.onVoiceIsolationProgress).mockResolvedValue(() => {})
    switchedOn(true)
  })
  afterEach(cleanup)

  it('is only a switch while off, and saves the switch', async () => {
    switchedOn(false)
    render(<VoiceIsolationSection />)
    await waitFor(() => expect(tauri.voiceIsolationStatus).toHaveBeenCalled())
    expect(screen.queryAllByRole('button')).toHaveLength(0)
    fireEvent.click(screen.getByRole('switch'))
    expect(useAppStore.getState().config.voice_isolation_enabled).toBe(true)
  })

  it('downloads the model when it is missing', async () => {
    status({ modelInstalled: false })
    render(<VoiceIsolationSection />)
    const download = await screen.findByRole('button', {
      name: 'settings.voiceIsolation.download(79 MB)',
    })
    status({})
    fireEvent.click(download)
    await waitFor(() => expect(tauri.voiceIsolationInstall).toHaveBeenCalled())
    await screen.findByRole('button', { name: 'settings.voiceIsolation.recordAgain' })
  })

  it('shows how much of the model has downloaded', async () => {
    status({ modelInstalled: false })
    let report: (progress: tauri.VoiceIsolationProgress) => void = () => {}
    vi.mocked(tauri.onVoiceIsolationProgress).mockImplementation(async (handler) => {
      report = handler
      return () => {}
    })
    vi.mocked(tauri.voiceIsolationInstall).mockReturnValue(new Promise(() => {}))
    render(<VoiceIsolationSection />)
    fireEvent.click(await screen.findByRole('button', { name: /download/ }))
    act(() => report({ downloaded: 39_542_462, total: 79_084_924, bytesPerSec: 2_000_000 }))
    expect((await screen.findByRole('progressbar')).getAttribute('aria-valuenow')).toBe('50')
    screen.getByText('40 MB / 79 MB (2 MB/s)')
  })

  it('picks up a download that is already running, and shows the result when it ends', async () => {
    status({ modelInstalled: false, downloading: true })
    let finish: () => void = () => {}
    vi.mocked(tauri.voiceIsolationInstall).mockReturnValue(
      new Promise<void>((resolve) => {
        finish = resolve
      }),
    )
    render(<VoiceIsolationSection />)
    const running = await screen.findByRole('button', {
      name: 'settings.voiceIsolation.downloading',
    })
    expect((running as HTMLButtonElement).disabled).toBe(true)
    status({})
    act(() => finish())
    await screen.findByRole('button', { name: 'settings.voiceIsolation.recordAgain' })
  })

  it('says so when the download fails', async () => {
    status({ modelInstalled: false })
    vi.mocked(tauri.voiceIsolationInstall).mockRejectedValue('network')
    render(<VoiceIsolationSection />)
    fireEvent.click(await screen.findByRole('button', { name: /download/ }))
    await screen.findByText('settings.voiceIsolation.downloadFailed')
  })

  it('keeps the next dictation as the voice sample when asked', async () => {
    status({ hasSample: false })
    render(<VoiceIsolationSection />)
    const record = await screen.findByRole('button', { name: 'settings.voiceIsolation.record' })
    status({ hasSample: false, enrollmentPending: true })
    fireEvent.click(record)
    await waitFor(() => expect(tauri.voiceIsolationEnrollNext).toHaveBeenCalledWith(true))
    await screen.findByText('settings.voiceIsolation.pending')
  })

  it('says that filtering waits for a voice sample', async () => {
    status({ hasSample: false })
    render(<VoiceIsolationSection />)
    await screen.findByText('settings.voiceIsolation.needsSample')
  })

  it('says nothing about a sample once there is one', async () => {
    render(<VoiceIsolationSection />)
    await screen.findByRole('button', { name: 'settings.voiceIsolation.recordAgain' })
    expect(screen.queryByText('settings.voiceIsolation.needsSample')).toBeNull()
  })

  it('says why the last recording was not kept', async () => {
    status({ hasSample: false, enrollmentError: 'too_short' })
    render(<VoiceIsolationSection />)
    await screen.findByText('settings.voiceIsolation.error.too_short')
  })

  it('deletes the voice sample', async () => {
    render(<VoiceIsolationSection />)
    const remove = await screen.findByRole('button', { name: 'settings.voiceIsolation.delete' })
    status({ hasSample: false })
    fireEvent.click(remove)
    await waitFor(() => expect(tauri.voiceIsolationDeleteSample).toHaveBeenCalled())
    await screen.findByRole('button', { name: 'settings.voiceIsolation.record' })
  })
})
