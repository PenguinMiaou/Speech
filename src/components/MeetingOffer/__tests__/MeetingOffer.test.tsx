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
})
