import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { MoreStep } from '../MoreStep'
import { bindingFromHotkey, useAppStore } from '../../../stores/appStore'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

describe('MoreStep', () => {
  afterEach(() => {
    cleanup()
    useAppStore.setState(useAppStore.getInitialState())
  })

  it('introduces meeting recording as record, transcript and minutes', () => {
    render(<MoreStep />)
    expect(screen.getByText('onboarding.more.meetingTitle', { selector: 'h3' })).toBeTruthy()
    const stages = screen.getAllByRole('listitem').map((item) => item.textContent)
    expect(stages).toEqual([
      'onboarding.more.record',
      'onboarding.more.transcript',
      'onboarding.more.minutes',
    ])
    expect(screen.getByText('onboarding.more.meetingDesc')).toBeTruthy()
  })

  it('introduces the Ask panel with its shortcut', () => {
    useAppStore.setState((state) => ({
      config: {
        ...state.config,
        hotkeys: { ...state.config.hotkeys, askBindings: [bindingFromHotkey('Alt+K')!] },
      },
    }))
    render(<MoreStep />)
    expect(screen.getByText('onboarding.more.askTitle')).toBeTruthy()
    const keys = Array.from(screen.getByTestId('ask-hotkey').querySelectorAll('kbd')).map(
      (key) => key.textContent,
    )
    expect(keys).toEqual(['Alt', 'K'])
    expect(screen.queryByText('onboarding.more.askNoHotkey')).toBeNull()
  })

  it('points to Settings when no Ask shortcut is set', () => {
    useAppStore.setState((state) => ({
      config: {
        ...state.config,
        ask_hotkey: '',
        hotkeys: { ...state.config.hotkeys, askBindings: [], ask: null },
      },
    }))
    render(<MoreStep />)
    expect(screen.queryByTestId('ask-hotkey')).toBeNull()
    expect(screen.getByText('onboarding.more.askNoHotkey')).toBeTruthy()
  })
})
