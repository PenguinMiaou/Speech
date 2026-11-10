import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
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

  it('offers to download only what changed and restarts into the new version', async () => {
    useAppStore.setState({ updateStatus: RELEASE })
    vi.mocked(tauri.updateInstall).mockResolvedValue()
    render(<UpdateSection />)

    fireEvent.click(screen.getByText('settings.update.install 1.2.1 15 MB'))

    await waitFor(() => expect(tauri.updateInstall).toHaveBeenCalled())
    expect(screen.getByText('settings.update.installing')).toBeTruthy()
  })

  it('opens the release page where the release is installed as a whole', () => {
    useAppStore.setState({ updateStatus: { ...RELEASE, patchBytes: null } })
    render(<UpdateSection />)

    fireEvent.click(screen.getByText('settings.update.openPage 1.2.1'))

    expect(openUrl).toHaveBeenCalledWith(RELEASE.releaseUrl)
    expect(tauri.updateInstall).not.toHaveBeenCalled()
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
