import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { MeetingAudio } from '../MeetingAudio'

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }))

afterEach(cleanup)

describe('MeetingAudio', () => {
  it('goes on from where it was when the same sound moves to another file', () => {
    const view = render(<MeetingAudio src="asset://m-1.wav" />)
    const player = view.container.querySelector('audio') as HTMLAudioElement
    const play = vi.fn(() => Promise.resolve())
    let at = 0
    Object.defineProperty(player, 'play', { value: play })
    Object.defineProperty(player, 'paused', { get: () => false, configurable: true })
    Object.defineProperty(player, 'currentTime', {
      get: () => at,
      set: (value: number) => (at = value),
      configurable: true,
    })
    at = 754.5

    view.rerender(<MeetingAudio src="asset://m-1.flac" />)
    expect(player.getAttribute('src')).toBe('asset://m-1.flac')
    // Reading the new file starts it from nothing; once it is known, it goes on.
    at = 0
    fireEvent.loadedMetadata(player)
    expect(at).toBe(754.5)
    expect(play).toHaveBeenCalledTimes(1)

    // A later reading of the same file is left alone.
    at = 3
    fireEvent.loadedMetadata(player)
    expect(at).toBe(3)
    expect(play).toHaveBeenCalledTimes(1)
  })

  it('moves back and on, changes speed, and answers Space and the arrows outside fields', () => {
    window.localStorage.clear()
    const view = render(
      <>
        <MeetingAudio src="asset://m-1.flac" />
        <input aria-label="notes" />
      </>,
    )
    const player = view.container.querySelector('audio') as HTMLAudioElement
    let at = 100
    const play = vi.fn(() => Promise.resolve())
    Object.defineProperty(player, 'play', { value: play })
    Object.defineProperty(player, 'paused', { get: () => true, configurable: true })
    Object.defineProperty(player, 'duration', { get: () => 120, configurable: true })
    Object.defineProperty(player, 'currentTime', {
      get: () => at,
      set: (value: number) => (at = value),
      configurable: true,
    })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.skipBack' }))
    expect(at).toBe(95)
    fireEvent.click(screen.getByRole('button', { name: 'meeting.skipOn' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.skipOn' }))
    expect(at).toBe(120)

    fireEvent.click(screen.getByRole('button', { name: 'meeting.rate' }))
    expect(player.playbackRate).toBe(1.25)
    expect(window.localStorage.getItem('meeting.rate')).toBe('1.25')

    fireEvent.keyDown(document.body, { key: 'ArrowLeft' })
    expect(at).toBe(115)
    fireEvent.keyDown(document.body, { key: ' ' })
    expect(play).toHaveBeenCalledTimes(1)
    // Typed in a field, the keys are the field's.
    fireEvent.keyDown(screen.getByLabelText('notes'), { key: ' ' })
    fireEvent.keyDown(screen.getByLabelText('notes'), { key: 'ArrowLeft' })
    expect(play).toHaveBeenCalledTimes(1)
    expect(at).toBe(115)
  })
})
