import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { HomePage } from '../index'
import { useAppStore } from '../../../stores/appStore'

vi.mock('../../../lib/tauri')
vi.mock('../MeetingCard', () => ({ MeetingCard: () => <div data-testid="meeting" /> }))
vi.mock('../TokenUsageCard', () => ({ TokenUsageCard: () => <div data-testid="tokens" /> }))
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, unknown>) =>
      values ? `${key} ${Object.values(values).join(' ')} end` : key,
  }),
}))

describe('HomePage', () => {
  beforeEach(() => {
    useAppStore.setState((state) => ({
      config: { ...state.config, hotkey: 'Ctrl+Shift+Space' },
      history: [],
    }))
  })
  afterEach(cleanup)

  it('opens with the hotkey, one keycap per key', () => {
    render(<HomePage />)
    const heading = screen.getByRole('heading', { level: 2 })
    const keys = Array.from(heading.querySelectorAll('kbd')).map((key) => key.textContent)
    expect(keys).toEqual(['Ctrl', 'Shift', 'Space'])
    expect(heading.textContent).toContain('home.speakWith')
    expect(heading.textContent).toContain('end')
  })

  it('shows the four figures in one band and leaves meetings to their own page', () => {
    render(<HomePage />)
    const band = screen.getByRole('group', { name: 'home.figures' })
    expect(band.querySelectorAll('dt')).toHaveLength(4)
    expect(screen.queryByTestId('meeting')).toBeNull()
    expect(screen.getByTestId('tokens')).toBeTruthy()
  })

  it('no longer repeats the sidebar as buttons', () => {
    render(<HomePage />)
    expect(screen.queryByRole('button', { name: 'nav.settings' })).toBeNull()
  })
})
