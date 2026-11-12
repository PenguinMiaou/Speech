import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useAppStore } from '../../../stores/appStore'
import * as tauri from '../../../lib/tauri'
import { UpdateSection } from '../UpdateSection'

vi.mock('../../../lib/tauri')
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }))
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, string>) =>
      params ? `${key} ${Object.values(params).join(' ')}` : key,
  }),
}))

const RELEASE = {
  current: '1.2.0',
  latest: '1.2.1',
  available: true,
  patchBytes: 15_000_000,
  releaseUrl: 'https://example.com/releases/tag/v1.2.1',
}

describe('UpdateSection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    localStorage.clear()
    useAppStore.setState({ updateStatus: null })
    vi.mocked(tauri.onUpdateProgress).mockResolvedValue(() => {})
  })
  afterEach(cleanup)

  it('says so when the installed version is the latest', async () => {
    vi.mocked(tauri.updateCheck).mockResolvedValue({
      ...RELEASE,
      latest: '1.2.0',
      available: false,
      patchBytes: null,
    })
    render(<UpdateSection />)

    fireEvent.click(screen.getByText('settings.update.check'))

    expect(await screen.findByText('settings.update.upToDate 1.2.0')).toBeTruthy()
  })

  it('shows how much of the changed files has been downloaded', async () => {
    useAppStore.setState({ updateStatus: RELEASE })
    let report: (progress: { downloaded: number; total: number }) => void = () => {}
    vi.mocked(tauri.onUpdateProgress).mockImplementation(async (handler) => {
      report = handler
      return () => {}
    })
    vi.mocked(tauri.updateDownload).mockReturnValue(new Promise(() => {}))
    render(<UpdateSection />)

    fireEvent.click(screen.getByText('settings.update.install 1.2.1 15 MB'))
    await waitFor(() => expect(tauri.updateDownload).toHaveBeenCalled())
    act(() => report({ downloaded: 6_000_000, total: 15_000_000 }))

    expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('40')
    expect(screen.getByText('6 MB / 15 MB')).toBeTruthy()
  })

  it('asks before restarting once the new version is in place', async () => {
    useAppStore.setState({ updateStatus: RELEASE })
    vi.mocked(tauri.onUpdateProgress).mockResolvedValue(() => {})
    vi.mocked(tauri.updateDownload).mockResolvedValue()
    vi.mocked(tauri.updateCheck).mockResolvedValue({ ...RELEASE, patchBytes: 0 })
    render(<UpdateSection />)

    fireEvent.click(screen.getByText('settings.update.install 1.2.1 15 MB'))

    expect(await screen.findByText('settings.update.ready 1.2.1')).toBeTruthy()
    expect(tauri.updateRestart).not.toHaveBeenCalled()
    fireEvent.click(screen.getByText('settings.update.restart'))
    expect(tauri.updateRestart).toHaveBeenCalled()
  })

  it('still offers the restart when the app was left open after downloading', () => {
    useAppStore.setState({ updateStatus: { ...RELEASE, patchBytes: 0 } })
    vi.mocked(tauri.onUpdateProgress).mockResolvedValue(() => {})
    render(<UpdateSection />)

    expect(screen.getByText('settings.update.ready 1.2.1')).toBeTruthy()
    expect(screen.queryByText(/settings\.update\.install/)).toBeNull()
  })

  it('opens the release page where the release is installed as a whole', () => {
    useAppStore.setState({ updateStatus: { ...RELEASE, patchBytes: null } })
    render(<UpdateSection />)

    fireEvent.click(screen.getByText('settings.update.openPage 1.2.1'))

    expect(openUrl).toHaveBeenCalledWith(RELEASE.releaseUrl)
    expect(tauri.updateDownload).not.toHaveBeenCalled()
  })

  it('offers the release page when the update could not be put in place', async () => {
    useAppStore.setState({ updateStatus: RELEASE })
    vi.mocked(tauri.updateDownload).mockRejectedValue('unwritable')
    render(<UpdateSection />)
    fireEvent.click(screen.getByText('settings.update.install 1.2.1 15 MB'))
    fireEvent.click(await screen.findByText('settings.update.openPage 1.2.1'))
    expect(openUrl).toHaveBeenCalledWith(RELEASE.releaseUrl)
  })

  it('reports a check or an update that failed', async () => {
    vi.mocked(tauri.updateCheck).mockRejectedValue('network')
    render(<UpdateSection />)
    fireEvent.click(screen.getByText('settings.update.check'))
    expect(await screen.findByText('settings.update.failed')).toBeTruthy()
  })

  it('remembers whether to check at launch', () => {
    render(<UpdateSection />)
    const auto = screen.getByRole('switch')
    expect(auto.getAttribute('aria-checked')).toBe('true')

    fireEvent.click(auto)

    expect(localStorage.getItem('update_auto_check')).toBe('off')
  })
})
