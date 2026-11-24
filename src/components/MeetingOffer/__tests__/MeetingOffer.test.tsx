import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as tauri from '../../../lib/tauri'
import { MeetingOffer } from '..'

vi.mock('../../../lib/tauri')
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, unknown>) =>
      values ? `${key} ${Object.values(values).join(' ')}` : key,
  }),
}))

const call: tauri.MeetingApp = { name: 'Office Chat', id: 'chat', browser: false, call: true }
let told: ((found: tauri.MeetingApp) => void) | null = null

describe('MeetingOffer', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(tauri.meetingOfferCurrent).mockResolvedValue(null)
    vi.mocked(tauri.onMeetingDetected).mockImplementation((handler) => {
      told = handler
      return Promise.resolve(() => {})
    })
  })
  afterEach(() => {
    cleanup()
    vi.useRealTimers()
  })

  it('asks before recording a call found going on, and records only on yes', async () => {
    render(<MeetingOffer />)
    await act(async () => told?.(call))
    expect(screen.getByRole('alertdialog').textContent).toContain('meeting.offerCall Office Chat')
    expect(tauri.meetingOfferTake).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.offerRecord' }))
    expect(tauri.meetingOfferTake).toHaveBeenCalled()
    expect(screen.queryByRole('alertdialog')).toBeNull()
  })

  it('shows the meeting already found when the window opens, and lets it go', async () => {
    vi.mocked(tauri.meetingOfferCurrent).mockResolvedValue({ ...call, call: false, name: 'Zoom' })
    render(<MeetingOffer />)
    expect((await screen.findByRole('alertdialog')).textContent).toContain(
      'meeting.offerMeeting Zoom',
    )
    fireEvent.click(screen.getAllByRole('button', { name: 'meeting.offerIgnore' })[0])
    expect(tauri.meetingOfferDismiss).toHaveBeenCalled()
    expect(tauri.meetingOfferTake).not.toHaveBeenCalled()
  })

  it('goes by itself when nobody answers', async () => {
    vi.useFakeTimers()
    render(<MeetingOffer />)
    await act(async () => told?.(call))
    await act(async () => vi.advanceTimersByTime(60_000))
    expect(screen.queryByRole('alertdialog')).toBeNull()
    expect(tauri.meetingOfferDismiss).toHaveBeenCalled()
  })

  it('shows a thin line that runs down over the minute the offer stays', async () => {
    const animate = vi.fn(() => ({ cancel: vi.fn() }))
    const original = (HTMLElement.prototype as { animate?: unknown }).animate
    Object.defineProperty(HTMLElement.prototype, 'animate', { value: animate, configurable: true })
    try {
      render(<MeetingOffer />)
      await act(async () => told?.(call))
      const bar = screen.getByTestId('offer-countdown')
      expect(bar.getAttribute('aria-hidden')).toBe('true')
      expect(animate).toHaveBeenCalledWith(
        [{ transform: 'scaleX(1)' }, { transform: 'scaleX(0)' }],
        expect.objectContaining({ duration: 60_000, fill: 'forwards' }),
      )
      const dialog = screen.getByRole('alertdialog')
      const note = document.getElementById(dialog.getAttribute('aria-describedby') ?? '')
      expect(note?.textContent).toBe('offer.closesByItself')
    } finally {
      Object.defineProperty(HTMLElement.prototype, 'animate', {
        value: original,
        configurable: true,
      })
    }
  })
})
