import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { MicrophoneSection } from '../MicrophoneSection'
import * as tauri from '../../../lib/tauri'
import { useAppStore } from '../../../stores/appStore'

vi.mock('../../../lib/tauri')
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, string>) =>
      values?.name ? `${key}(${values.name})` : key,
  }),
}))

const DEVICES = [
  { id: 'built-in', name: 'MacBook Air Microphone', is_default: true },
  { id: 'usb', name: 'USB Microphone', is_default: false },
]

function useMicrophone(input_device: string, mute_system_audio = true) {
  useAppStore.setState((state) => ({
    config: { ...state.config, input_device, mute_system_audio },
  }))
}

describe('MicrophoneSection', () => {
  beforeEach(() => {
    vi.mocked(tauri.listInputDevices).mockResolvedValue(DEVICES)
    useMicrophone('')
  })
  afterEach(cleanup)

  it('offers the system default and every microphone, and saves the pick', async () => {
    render(<MicrophoneSection showMute />)
    const trigger = screen.getByRole('combobox', { name: 'settings.microphoneInput' })
    await screen.findByText('settings.systemDefaultMicrophone(MacBook Air Microphone)')
    fireEvent.click(trigger)
    expect(screen.getAllByRole('option').map((option) => option.textContent)).toEqual([
      'settings.systemDefaultMicrophone(MacBook Air Microphone)',
      'MacBook Air Microphone',
      'USB Microphone',
    ])
    fireEvent.click(screen.getByRole('option', { name: 'USB Microphone' }))
    expect(useAppStore.getState().config.input_device).toBe('usb')
  })

  it('says when the chosen microphone is not connected', async () => {
    useMicrophone('unplugged')
    render(<MicrophoneSection showMute />)
    const trigger = screen.getByRole('combobox', { name: 'settings.microphoneInput' })
    await screen.findByText('settings.microphoneNotConnected')
    expect(trigger.textContent).toContain('settings.microphoneNotConnected')
  })

  it('turns muting off', () => {
    render(<MicrophoneSection showMute />)
    fireEvent.click(screen.getByRole('switch'))
    expect(useAppStore.getState().config.mute_system_audio).toBe(false)
  })

  it('hides the mute switch where muting is not supported yet', () => {
    render(<MicrophoneSection showMute={false} />)
    expect(screen.queryByRole('switch')).toBeNull()
  })
})
