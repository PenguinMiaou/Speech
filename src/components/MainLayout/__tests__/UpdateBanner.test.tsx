import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { UpdateBanner } from '../UpdateBanner'
import { useAppStore } from '../../../stores/appStore'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, unknown>) =>
      values ? `${key} ${Object.values(values).join(' ')}` : key,
  }),
}))

const RELEASE = {
  current: '1.2.1',
  latest: '1.2.2',
  available: true,
  patchBytes: 15_000_000,
  releaseUrl: 'https://example.com/releases/v1.2.2',
}

describe('UpdateBanner', () => {
  beforeEach(() => {
    window.location.hash = ''
    useAppStore.setState({ updateStatus: null })
  })
  afterEach(cleanup)

  it('stays out of the way when there is nothing newer', () => {
    useAppStore.setState({ updateStatus: { ...RELEASE, latest: '1.2.1', available: false } })
    render(<UpdateBanner />)
    expect(screen.queryByText(/settings\.update\.banner/)).toBeNull()
  })

  it('names the new version and leads to the About page', () => {
    useAppStore.setState({ updateStatus: RELEASE })
    render(<UpdateBanner />)

    expect(screen.getByText('settings.update.banner 1.2.2')).toBeTruthy()
    fireEvent.click(screen.getByText('settings.update.bannerAction'))

    expect(window.location.hash).toBe('#/settings?pane=about')
  })

  it('can be closed, and comes back for a later version', () => {
    useAppStore.setState({ updateStatus: RELEASE })
    render(<UpdateBanner />)

    fireEvent.click(screen.getByRole('button', { name: 'common.close' }))
    expect(screen.queryByText(/settings\.update\.banner/)).toBeNull()

    act(() => useAppStore.setState({ updateStatus: { ...RELEASE, latest: '1.2.3' } }))
    expect(screen.getByText('settings.update.banner 1.2.3')).toBeTruthy()
  })
})
