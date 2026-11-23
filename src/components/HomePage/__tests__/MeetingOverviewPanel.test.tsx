import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MeetingOverviewPanel } from '../MeetingOverviewPanel'
import { meetingOverview } from '../../../lib/tauri'

vi.mock('../../../lib/tauri', () => ({ meetingOverview: vi.fn() }))
vi.mock('../../../stores/meetingStore', () => ({ minutesLanguage: () => '' }))
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, string>) =>
      values?.time ? `${key} ${values.time}` : key,
  }),
}))

const asked = vi.mocked(meetingOverview)
const TEXT = 'Speaker 1  00:00\nHello.\n\nSpeaker 2  01:05\nThe roof leaks.'
const MADE = {
  chars: 0,
  chapters: [
    { at_s: 0, title: 'Opening', summary: 'They say hello.' },
    { at_s: 65, title: 'Roof', summary: 'The roof leaks.' },
  ],
  speakers: [
    { speaker: 1, summary: 'Opened the meeting.' },
    { speaker: 2, summary: 'Raised the roof.' },
  ],
}

const props = {
  meetingId: 'm-1',
  transcript: TEXT,
  label: 'Speaker',
  names: { 2: 'Ann' } as Record<number, string>,
  overview: null,
  active: true,
  busy: false,
}

describe('MeetingOverviewPanel', () => {
  beforeEach(() => asked.mockReset())
  afterEach(cleanup)

  it('is asked for only once it is shown, and not while the meeting is busy', () => {
    const { rerender } = render(
      <MeetingOverviewPanel {...props} active={false} onOverview={vi.fn()} />,
    )
    rerender(<MeetingOverviewPanel {...props} busy onOverview={vi.fn()} />)
    expect(asked).not.toHaveBeenCalled()
  })

  it('keeps what was made with how long the text was', async () => {
    asked.mockResolvedValueOnce(MADE)
    const onOverview = vi.fn()
    render(<MeetingOverviewPanel {...props} onOverview={onOverview} />)
    expect(asked).toHaveBeenCalledWith(TEXT, 'Speaker', { 2: 'Ann' }, '')
    await waitFor(() => expect(onOverview).toHaveBeenCalledWith({ ...MADE, chars: TEXT.length }))
  })

  it('shows the chapters with their times and each speaker by name where given', () => {
    const onSeek = vi.fn()
    render(
      <MeetingOverviewPanel
        {...props}
        overview={{ ...MADE, chars: TEXT.length }}
        onOverview={vi.fn()}
        onSeek={onSeek}
      />,
    )
    expect(asked).not.toHaveBeenCalled()
    expect(screen.getByText('Roof')).toBeDefined()
    expect(screen.getByText('Speaker 1')).toBeDefined()
    expect(screen.getByText('Ann')).toBeDefined()
    expect(screen.queryByText('meeting.overviewStale')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.playFrom 01:05' }))
    expect(onSeek).toHaveBeenCalledWith(65)
  })

  it('tells when the text changed after the overview was made', () => {
    render(
      <MeetingOverviewPanel {...props} overview={{ ...MADE, chars: 3 }} onOverview={vi.fn()} />,
    )
    expect(screen.getByText('meeting.overviewStale')).toBeDefined()
  })

  it('does not ask again by itself after a failure, only when told to', async () => {
    asked.mockRejectedValueOnce('llm_http_500')
    const { rerender } = render(<MeetingOverviewPanel {...props} onOverview={vi.fn()} />)
    expect(await screen.findByRole('alert')).toHaveTextContent('meeting.error.overview')
    rerender(<MeetingOverviewPanel {...props} active={false} onOverview={vi.fn()} />)
    rerender(<MeetingOverviewPanel {...props} onOverview={vi.fn()} />)
    expect(asked).toHaveBeenCalledTimes(1)
    asked.mockResolvedValueOnce(MADE)
    fireEvent.click(screen.getByRole('button', { name: 'meeting.overviewAgain' }))
    expect(asked).toHaveBeenCalledTimes(2)
  })

  it('drops an overview of a text that has changed since it was asked for', async () => {
    let answer: (made: typeof MADE) => void = () => {}
    asked.mockReturnValueOnce(new Promise((resolve) => (answer = resolve)))
    const onOverview = vi.fn()
    const { rerender } = render(<MeetingOverviewPanel {...props} onOverview={onOverview} />)
    rerender(
      <MeetingOverviewPanel {...props} transcript={`${TEXT} more`} onOverview={onOverview} />,
    )
    answer(MADE)
    await waitFor(() => expect(asked).toHaveBeenCalledTimes(1))
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(onOverview).not.toHaveBeenCalled()
  })
})
