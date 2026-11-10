import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { AdvancedExtras } from '../AdvancedExtras'
import { useAppStore } from '../../../stores/appStore'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

function configure(patch: Record<string, unknown>) {
  const { config } = useAppStore.getState()
  useAppStore.setState({ config: { ...config, ...patch } })
}
const config = () => useAppStore.getState().config

function onPlatform(name: string) {
  Object.defineProperty(navigator, 'platform', { value: name, configurable: true })
}

describe('AdvancedExtras', () => {
  beforeEach(() => {
    onPlatform('MacIntel')
    configure({
      streaming_insert_enabled: false,
      output_mode: 'keyboard',
      restore_clipboard_after_paste: true,
      history_enabled: true,
      history_retention_days: 0,
      history_max_entries: 5000,
    })
  })
  afterEach(() => {
    cleanup()
    Reflect.deleteProperty(navigator, 'platform')
  })

  it('switches on writing while the model is still generating', () => {
    render(<AdvancedExtras />)
    fireEvent.click(screen.getByRole('switch', { name: 'settings.streamingInsert' }))
    expect(config().streaming_insert_enabled).toBe(true)
  })

  it('offers the clipboard choices only when text is pasted', () => {
    render(<AdvancedExtras />)
    expect(screen.queryByRole('switch', { name: 'settings.restoreClipboardAfterPaste' })).toBeNull()
    cleanup()

    configure({ output_mode: 'clipboard' })
    render(<AdvancedExtras />)
    fireEvent.click(screen.getByRole('switch', { name: 'settings.restoreClipboardAfterPaste' }))
    expect(config().restore_clipboard_after_paste).toBe(false)
    // The paste shortcut is always Command+V on a Mac.
    expect(screen.queryByRole('combobox', { name: 'settings.pasteShortcut' })).toBeNull()
  })

  it('offers the paste shortcut and the newline key on Windows', () => {
    onPlatform('Win32')
    configure({ output_mode: 'clipboard' })
    render(<AdvancedExtras />)
    expect(screen.getByRole('combobox', { name: 'settings.pasteShortcut' })).toBeTruthy()
    expect(
      screen.getByRole('combobox', { name: 'settings.windowsSendInputNewlineMode' }),
    ).toBeTruthy()
  })

  it('sets how long and how much history is kept', () => {
    render(<AdvancedExtras />)
    fireEvent.change(screen.getByLabelText('settings.historyRetentionDays'), {
      target: { value: '30' },
    })
    fireEvent.change(screen.getByLabelText('settings.historyMaxEntries'), {
      target: { value: '200' },
    })
    expect(config().history_retention_days).toBe(30)
    expect(config().history_max_entries).toBe(200)
  })

  it('keeps the history numbers sane and hides them when history is off', () => {
    render(<AdvancedExtras />)
    fireEvent.change(screen.getByLabelText('settings.historyRetentionDays'), {
      target: { value: '-4' },
    })
    fireEvent.change(screen.getByLabelText('settings.historyMaxEntries'), {
      target: { value: '' },
    })
    expect(config().history_retention_days).toBe(0)
    expect(config().history_max_entries).toBe(1)
    cleanup()

    configure({ history_enabled: false })
    render(<AdvancedExtras />)
    expect(screen.queryByLabelText('settings.historyMaxEntries')).toBeNull()
  })
})
