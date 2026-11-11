import { useState } from 'react'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { MeetingMinutes } from '../MeetingMinutes'
import * as tauri from '../../../lib/tauri'

vi.mock('../../../lib/tauri')
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

function Held() {
  const [minutes, setMinutes] = useState<string | null>(null)
  return (
    <MeetingMinutes transcript="what was said" notes="" minutes={minutes} onMinutes={setMinutes} />
  )
}

async function ready(templates: tauri.MinutesTemplate[] = []) {
  vi.mocked(tauri.minutesTemplates).mockResolvedValue(templates)
  render(<Held />)
  await waitFor(() => expect(tauri.minutesTemplates).toHaveBeenCalled())
}

describe('MeetingMinutes', () => {
  beforeEach(() => vi.clearAllMocks())
  afterEach(cleanup)

  it('writes minutes after the built-in template', async () => {
    vi.mocked(tauri.meetingMinutes).mockResolvedValue('the minutes')
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.minutes' }))
    await screen.findByDisplayValue('the minutes')
    expect(tauri.meetingMinutes).toHaveBeenCalledWith(
      'what was said',
      'meeting.defaultTemplate',
      '',
    )
  })

  it('uses an imported template once it is chosen', async () => {
    vi.mocked(tauri.importMinutesTemplate).mockResolvedValue([{ name: 'weekly', text: '## Done' }])
    vi.mocked(tauri.meetingMinutes).mockResolvedValue('minutes')
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.importTemplate' }))
    await screen.findByRole('button', { name: 'meeting.removeTemplate' })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.minutes' }))
    await screen.findByDisplayValue('minutes')
    expect(tauri.meetingMinutes).toHaveBeenCalledWith('what was said', '## Done', '')
  })

  it('goes back to the built-in template when the chosen one is removed', async () => {
    vi.mocked(tauri.importMinutesTemplate).mockResolvedValue([{ name: 'weekly', text: '## Done' }])
    vi.mocked(tauri.removeMinutesTemplate).mockResolvedValue([])
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.importTemplate' }))
    fireEvent.click(await screen.findByRole('button', { name: 'meeting.removeTemplate' }))
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: 'meeting.removeTemplate' })).toBeNull(),
    )
    expect(tauri.removeMinutesTemplate).toHaveBeenCalledWith('weekly')
  })

  it('answers a question about the meeting', async () => {
    vi.mocked(tauri.meetingAsk).mockResolvedValue('on Friday')
    await ready()
    fireEvent.change(screen.getByLabelText('meeting.ask'), { target: { value: ' when? ' } })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.askButton' }))
    expect((await screen.findByRole('status')).textContent).toBe('on Friday')
    expect(tauri.meetingAsk).toHaveBeenCalledWith('what was said', 'when?')
  })

  it('does not ask an empty question', async () => {
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.askButton' }))
    expect(tauri.meetingAsk).not.toHaveBeenCalled()
  })

  it('says so when no language model is set up', async () => {
    vi.mocked(tauri.meetingMinutes).mockRejectedValue('llm_not_configured')
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.minutes' }))
    await screen.findByText('meeting.error.llm_not_configured')
  })

  it('copies and saves the minutes', async () => {
    vi.mocked(tauri.meetingMinutes).mockResolvedValue('the minutes')
    vi.mocked(tauri.copyText).mockResolvedValue()
    vi.mocked(tauri.saveTextFile).mockResolvedValue(true)
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.minutes' }))
    await screen.findByDisplayValue('the minutes')
    fireEvent.click(screen.getByText('meeting.copy'))
    expect(tauri.copyText).toHaveBeenCalledWith('the minutes')
    fireEvent.click(screen.getByText('meeting.save'))
    expect(tauri.saveTextFile).toHaveBeenCalledWith('the minutes', 'meeting.minutesFileName')
  })
})
