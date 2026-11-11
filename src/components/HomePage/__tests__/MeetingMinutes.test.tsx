import { useState } from 'react'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { MeetingMinutes } from '../MeetingMinutes'
import * as tauri from '../../../lib/tauri'

vi.mock('../../../lib/tauri')
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

const seek = vi.fn()

function Held({ written = null }: { written?: string | null }) {
  const [minutes, setMinutes] = useState<string | null>(written)
  return (
    <MeetingMinutes
      transcript="what was said"
      notes=""
      minutes={minutes}
      onMinutes={setMinutes}
      onEdit={setMinutes}
      onSeek={seek}
    />
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
    await screen.findByText('the minutes')
    expect(tauri.meetingMinutes).toHaveBeenCalledWith(
      'what was said',
      'meeting.defaultTemplate',
      '',
    )
  })

  it.each(['chapters', 'oneOnOne', 'interview', 'talk', 'standup'])(
    'writes minutes after the %s template that comes with the app',
    async (kind) => {
      vi.mocked(tauri.meetingMinutes).mockResolvedValue('the minutes')
      await ready()
      fireEvent.click(screen.getByRole('combobox', { name: 'meeting.template' }))
      fireEvent.click(screen.getByRole('option', { name: `meeting.${kind}TemplateName` }))
      fireEvent.click(screen.getByRole('button', { name: 'meeting.minutes' }))
      await screen.findByText('the minutes')
      expect(tauri.meetingMinutes).toHaveBeenCalledWith(
        'what was said',
        `meeting.${kind}Template`,
        '',
      )
      expect(screen.queryByRole('button', { name: 'meeting.removeTemplate' })).toBeNull()
    },
  )

  it('uses an imported template once it is chosen', async () => {
    vi.mocked(tauri.importMinutesTemplate).mockResolvedValue([{ name: 'weekly', text: '## Done' }])
    vi.mocked(tauri.meetingMinutes).mockResolvedValue('minutes')
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.importTemplate' }))
    await screen.findByRole('button', { name: 'meeting.removeTemplate' })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.minutes' }))
    await screen.findByText('minutes')
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
    await screen.findByText('the minutes')
    fireEvent.click(screen.getByText('meeting.copy'))
    expect(tauri.copyText).toHaveBeenCalledWith('the minutes')
    fireEvent.click(screen.getByText('meeting.save'))
    expect(tauri.saveTextFile).toHaveBeenCalledWith('the minutes', 'meeting.minutesFileName')
  })

  it('leads from a time in the minutes to that moment of the sound', async () => {
    vi.mocked(tauri.minutesTemplates).mockResolvedValue([])
    render(<Held written={'## Decisions\nCut the budget [01:05]\nShip late [1:02:10].'} />)
    fireEvent.click(screen.getByRole('button', { name: '01:05' }))
    expect(seek).toHaveBeenLastCalledWith(65)
    fireEvent.click(screen.getByRole('button', { name: '1:02:10' }))
    expect(seek).toHaveBeenLastCalledWith(3730)
  })

  it('ticks a task off and keeps that in the minutes', async () => {
    vi.mocked(tauri.minutesTemplates).mockResolvedValue([])
    render(<Held written={'- [ ] Send the plan — Ann\n- [x] Book a room'} />)
    const boxes = screen.getAllByRole('checkbox') as HTMLInputElement[]
    expect(boxes.map((box) => box.checked)).toEqual([false, true])
    fireEvent.click(boxes[0])
    expect((screen.getAllByRole('checkbox')[0] as HTMLInputElement).checked).toBe(true)
    fireEvent.click(screen.getByRole('button', { name: 'meeting.editMinutes' }))
    expect((screen.getByLabelText('meeting.minutesResult') as HTMLTextAreaElement).value).toBe(
      '- [x] Send the plan — Ann\n- [x] Book a room',
    )
  })

  it('lets the minutes be changed by hand', async () => {
    vi.mocked(tauri.minutesTemplates).mockResolvedValue([])
    render(<Held written="the minutes" />)
    fireEvent.click(screen.getByRole('button', { name: 'meeting.editMinutes' }))
    const box = screen.getByLabelText('meeting.minutesResult')
    fireEvent.change(box, { target: { value: 'my minutes' } })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.editMinutes' }))
    await screen.findByText('my minutes')
  })
})
