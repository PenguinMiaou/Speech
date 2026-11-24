import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { HomePage } from '../index'
import { bindingFromHotkey, useAppStore } from '../../../stores/appStore'
import { meetingsList } from '../../../lib/tauri'

vi.mock('../../../lib/tauri')
vi.mock('../MeetingCard', () => ({ MeetingCard: () => <div data-testid="meeting" /> }))
vi.mock('../TokenUsageCard', () => ({ TokenUsageCard: () => <div data-testid="tokens" /> }))
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, unknown>) =>
      values ? `${key} ${Object.values(values).join(' ')} end` : key,
    i18n: { language: 'en' },
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

  it('names the recognizer and the text output as Settings does, not by their ids', () => {
    useAppStore.setState((state) => ({
      config: { ...state.config, stt_provider: 'deepgram', output_mode: 'clipboard' },
    }))
    render(<HomePage />)
    expect(screen.getByText('providers.stt.deepgram')).toBeTruthy()
    expect(screen.getByText('settings.clipboardPaste')).toBeTruthy()
    expect(screen.queryByText('clipboard')).toBeNull()
  })

  it('lists the dictation, Ask and translate shortcuts', () => {
    useAppStore.setState((state) => ({
      config: {
        ...state.config,
        hotkeys: {
          ...state.config.hotkeys,
          dictationBindings: [bindingFromHotkey('Ctrl+Shift+Space')!],
          askBindings: [bindingFromHotkey('Alt+K')!],
          translateBindings: [],
          translate: null,
        },
      },
    }))
    render(<HomePage />)
    const list = screen.getByLabelText('home.shortcuts')
    expect(list.textContent).toContain('settings.dictationHotkey')
    expect(list.textContent).toContain('settings.askHotkey')
    expect(list.textContent).toContain('settings.askHotkeyAltK')
    expect(list.textContent).toContain('settings.translateHotkeyhome.disabled')
  })

  it('shows the three latest meetings and opens the meetings page from them', async () => {
    vi.mocked(meetingsList).mockResolvedValue([
      { id: 'a', started_at: 1_000, preview: 'first', has_minutes: false, title: 'Oldest' },
      { id: 'b', started_at: 4_000, preview: 'the plan', has_minutes: true, title: 'Weekly' },
      { id: 'c', started_at: 3_000, preview: 'notes', has_minutes: false },
      { id: 'd', started_at: 2_000, preview: '', has_minutes: false, duration_s: 600 },
    ])
    window.location.hash = '#/'
    render(<HomePage />)
    const weekly = await screen.findByRole('button', { name: /Weekly/ })
    const items = screen.getAllByRole('listitem').map((item) => item.textContent)
    expect(items).toHaveLength(3)
    expect(items[0]).toContain('Weekly')
    expect(items[1]).toContain('notes')
    expect(items[2]).toContain('home.untitledMeeting')
    expect(items[2]).toContain('home.meetingMinutes 10 end')
    fireEvent.click(weekly)
    expect(window.location.hash).toBe('#/meeting')
  })

  it('starts a meeting by opening the meetings page, without recording from here', async () => {
    vi.mocked(meetingsList).mockResolvedValue([])
    window.location.hash = '#/'
    render(<HomePage />)
    expect(await screen.findByText('home.noMeetings')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'home.startMeeting' }))
    expect(window.location.hash).toBe('#/meeting')
  })
})
