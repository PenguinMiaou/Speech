import { useState } from 'react'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
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
  const [answers, setAnswers] = useState<tauri.MeetingAnswer[]>([])
  return (
    <MeetingMinutes
      transcript="what was said"
      notes=""
      minutes={minutes}
      onMinutes={setMinutes}
      onEdit={setMinutes}
      onSeek={seek}
      answers={answers}
      onAnswers={setAnswers}
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
      false,
      '',
    )
  })

  it.each(['chapters', 'oneOnOne', 'interview', 'talk', 'standup'])(
    'writes minutes after the %s template that comes with the app',
    async (kind) => {
      vi.mocked(tauri.meetingMinutes).mockResolvedValue('the minutes')
      await ready()
      fireEvent.click(screen.getByRole('button', { name: 'meeting.template' }))
      const gallery = within(screen.getByRole('dialog'))
      fireEvent.click(gallery.getByRole('radio', { name: `meeting.${kind}TemplateName` }))
      fireEvent.click(gallery.getByRole('button', { name: 'meeting.minutes' }))
      await screen.findByText('the minutes')
      expect(tauri.meetingMinutes).toHaveBeenCalledWith(
        'what was said',
        `meeting.${kind}Template`,
        '',
        false,
        '',
      )
      expect(screen.queryByRole('dialog')).toBeNull()
      // The template stays picked, and its name stands on the way to the others.
      expect(screen.getByRole('button', { name: 'meeting.template' }).textContent).toContain(
        `meeting.${kind}TemplateName`,
      )
    },
  )

  it('uses an imported template once it is chosen', async () => {
    vi.mocked(tauri.importMinutesTemplate).mockResolvedValue([{ name: 'weekly', text: '## Done' }])
    vi.mocked(tauri.meetingMinutes).mockResolvedValue('minutes')
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.template' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.importTemplate' }))
    await screen.findByRole('radio', { name: 'weekly' })
    fireEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', { name: 'meeting.minutes' }),
    )
    await screen.findByText('minutes')
    expect(tauri.meetingMinutes).toHaveBeenCalledWith('what was said', '## Done', '', false, '')
  })

  it('goes back to the built-in template when the chosen one is removed', async () => {
    vi.mocked(tauri.importMinutesTemplate).mockResolvedValue([{ name: 'weekly', text: '## Done' }])
    vi.mocked(tauri.removeMinutesTemplate).mockResolvedValue([])
    vi.mocked(tauri.meetingMinutes).mockResolvedValue('minutes')
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.template' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.importTemplate' }))
    fireEvent.click(await screen.findByRole('button', { name: 'meeting.removeTemplate weekly' }))
    await waitFor(() => expect(screen.queryByRole('radio', { name: 'weekly' })).toBeNull())
    expect(tauri.removeMinutesTemplate).toHaveBeenCalledWith('weekly')
    fireEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', { name: 'meeting.minutes' }),
    )
    await screen.findByText('minutes')
    expect(tauri.meetingMinutes).toHaveBeenCalledWith(
      'what was said',
      'meeting.defaultTemplate',
      '',
      false,
      '',
    )
  })

  it('sets strong words and counted lines of the minutes out', async () => {
    render(<Held written={'## Decided\n1. **Ship Friday** after tests\n- hold the rest'} />)
    expect(screen.getByText('Ship Friday').tagName).toBe('STRONG')
    expect(screen.queryByText(/\*\*/)).toBeNull()
  })

  it('answers a question about the meeting', async () => {
    vi.mocked(tauri.meetingAsk).mockResolvedValue('on Friday')
    await ready()
    fireEvent.change(screen.getByLabelText('meeting.ask'), { target: { value: ' when? ' } })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.askButton' }))
    expect((await screen.findByRole('status')).textContent).toBe('on Friday')
    expect(tauri.meetingAsk).toHaveBeenCalledWith('what was said', 'when?')
  })

  it('keeps each answer with its question, the newest first, and lets one go', async () => {
    vi.mocked(tauri.meetingAsk).mockResolvedValueOnce('on Friday').mockResolvedValueOnce('Ann')
    await ready()
    fireEvent.change(screen.getByLabelText('meeting.ask'), { target: { value: 'when?' } })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.askButton' }))
    await screen.findByText('on Friday')
    fireEvent.change(screen.getByLabelText('meeting.ask'), { target: { value: 'who?' } })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.askButton' }))
    await screen.findByText('Ann')
    const kept = within(screen.getByRole('list', { name: 'meeting.answers' }))
    expect(kept.getAllByRole('listitem').map((item) => item.textContent)).toEqual([
      'who?Ann',
      'when?on Friday',
    ])
    fireEvent.click(kept.getAllByRole('button', { name: 'meeting.answerForget' })[1])
    expect(screen.queryByText('on Friday')).toBeNull()
  })

  it("asks a question that comes with the app, and keeps the user's own to ask again", async () => {
    vi.mocked(tauri.meetingAsk).mockResolvedValue('the email')
    vi.mocked(tauri.meetingQuestions).mockResolvedValue([])
    vi.mocked(tauri.meetingQuestionsSave).mockImplementation((all) => Promise.resolve(all))
    await ready()
    fireEvent.click(screen.getByRole('button', { name: 'meeting.questionFollowUp' }))
    await screen.findByText('the email')
    expect(tauri.meetingAsk).toHaveBeenCalledWith('what was said', 'meeting.questionFollowUp')
    fireEvent.change(screen.getByLabelText('meeting.ask'), { target: { value: 'any risks?' } })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.questionKeep' }))
    expect(tauri.meetingQuestionsSave).toHaveBeenCalledWith(['any risks?'])
    const kept = await screen.findByRole('button', { name: 'any risks?' })
    fireEvent.click(kept)
    await waitFor(() =>
      expect(tauri.meetingAsk).toHaveBeenLastCalledWith('what was said', 'any risks?'),
    )
    fireEvent.click(screen.getByRole('button', { name: 'meeting.questionForget' }))
    expect(tauri.meetingQuestionsSave).toHaveBeenLastCalledWith([])
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

  it('learns from a change by hand how the minutes should be written', async () => {
    vi.mocked(tauri.minutesTemplates).mockResolvedValue([])
    vi.mocked(tauri.minutesStyle).mockResolvedValue([])
    vi.mocked(tauri.minutesLearn).mockResolvedValue(['One line per point', 'No background'])
    vi.mocked(tauri.minutesStyleSave).mockResolvedValue(['No background'])
    render(<Held written={'# Minutes\n- a long point'} />)
    fireEvent.click(screen.getByRole('button', { name: 'meeting.editMinutes' }))
    fireEvent.change(screen.getByLabelText('meeting.minutesResult'), {
      target: { value: '# Minutes\n- short' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.editMinutes' }))
    expect(tauri.minutesLearn).toHaveBeenCalledWith(
      '# Minutes\n- a long point',
      '# Minutes\n- short',
    )
    fireEvent.click(await screen.findByRole('button', { name: 'meeting.style' }))
    expect(screen.getByText('One line per point')).toBeTruthy()
    fireEvent.click(screen.getAllByRole('button', { name: 'meeting.styleForget' })[0])
    expect(tauri.minutesStyleSave).toHaveBeenCalledWith('# Minutes', ['No background'])
    expect(screen.queryByText('One line per point')).toBeNull()
  })

  it('writes full minutes once full is chosen, and remembers the choice', async () => {
    vi.mocked(tauri.meetingMinutes).mockResolvedValue('the minutes')
    await ready()
    const brief = screen.getByRole('radio', { name: 'meeting.lengthBrief' })
    expect(brief.getAttribute('aria-checked')).toBe('true')
    fireEvent.click(screen.getByRole('radio', { name: 'meeting.lengthFull' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.minutes' }))
    await screen.findByText('the minutes')
    expect(tauri.meetingMinutes).toHaveBeenCalledWith(
      'what was said',
      'meeting.defaultTemplate',
      '',
      true,
      '',
    )
    expect(localStorage.getItem('meeting.minutesFull')).toBe('1')
    localStorage.removeItem('meeting.minutesFull')
  })

  it('writes the minutes in the language picked, and remembers it', async () => {
    vi.mocked(tauri.meetingMinutes).mockResolvedValue('the minutes')
    await ready()
    const language = screen.getByRole('combobox', { name: 'meeting.minutesLanguage' })
    // By default the minutes are written in the language the meeting was held in.
    expect((language as HTMLSelectElement).value).toBe('')
    fireEvent.change(language, { target: { value: 'en' } })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.minutes' }))
    await screen.findByText('the minutes')
    expect(tauri.meetingMinutes).toHaveBeenCalledWith(
      'what was said',
      'meeting.defaultTemplate',
      '',
      false,
      'en',
    )
    expect(localStorage.getItem('meeting.minutesLanguage')).toBe('en')
    localStorage.removeItem('meeting.minutesLanguage')
  })

  it('learns nothing when the minutes were opened and left as they were', async () => {
    vi.mocked(tauri.minutesTemplates).mockResolvedValue([])
    render(<Held written="the minutes" />)
    fireEvent.click(screen.getByRole('button', { name: 'meeting.editMinutes' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.editMinutes' }))
    expect(tauri.minutesLearn).not.toHaveBeenCalled()
  })
})
