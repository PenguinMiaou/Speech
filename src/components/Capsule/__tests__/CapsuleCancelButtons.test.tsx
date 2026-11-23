import React from 'react'
import { render, screen, fireEvent, cleanup } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { CapsuleRecording } from '../CapsuleRecording'
import { CapsuleProcessing } from '../CapsuleProcessing'
import { abortRecording } from '../../../lib/tauri'
import { useAppStore } from '../../../stores/appStore'

vi.mock('framer-motion', () => ({
  motion: new Proxy(
    {},
    {
      get:
        (_target, tag: string) =>
        ({ children, ...props }: React.HTMLAttributes<HTMLElement>) =>
          React.createElement(tag, props, children),
    },
  ),
  useReducedMotion: () => true,
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}))

vi.mock('../../../lib/tauri', () => ({
  abortRecording: vi.fn().mockResolvedValue(undefined),
}))

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

describe('Capsule cancel buttons', () => {
  it('does not let recording cancel pointer events reach the capsule shell', () => {
    const onPointerDown = vi.fn()
    const onPointerUp = vi.fn()

    render(
      <div onPointerDown={onPointerDown} onPointerUp={onPointerUp}>
        <CapsuleRecording />
      </div>,
    )

    const button = screen.getByRole('button', { name: 'capsule.cancelRecording' })
    fireEvent.pointerDown(button)
    fireEvent.pointerUp(button)
    fireEvent.click(button)

    expect(onPointerDown).not.toHaveBeenCalled()
    expect(onPointerUp).not.toHaveBeenCalled()
    expect(abortRecording).toHaveBeenCalledTimes(1)
  })

  it('does not let processing cancel pointer events reach the capsule shell', () => {
    const onPointerDown = vi.fn()
    const onPointerUp = vi.fn()

    render(
      <div onPointerDown={onPointerDown} onPointerUp={onPointerUp}>
        <CapsuleProcessing />
      </div>,
    )

    const button = screen.getByRole('button', { name: 'capsule.cancelProcessing' })
    fireEvent.pointerDown(button)
    fireEvent.pointerUp(button)
    fireEvent.click(button)

    expect(onPointerDown).not.toHaveBeenCalled()
    expect(onPointerUp).not.toHaveBeenCalled()
    expect(abortRecording).toHaveBeenCalledTimes(1)
  })

  it('shows a microphone that hears nothing at all in place of the wave, until the dictation ends', () => {
    useAppStore.getState().setPipelineState('recording')
    useAppStore.getState().setMicSilent(true)
    render(<CapsuleRecording />)
    expect(screen.getByRole('alert').getAttribute('aria-label')).toBe('capsule.errors.micSilent')
    useAppStore.getState().setPipelineState('idle')
    expect(useAppStore.getState().micSilent).toBe(false)
  })
})
