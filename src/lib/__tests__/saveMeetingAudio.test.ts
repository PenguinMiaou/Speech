import { beforeEach, describe, expect, it, vi } from 'vitest'

const save = vi.fn()
const invoke = vi.fn()
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: (...args: unknown[]) => save(...args) }))
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invoke(...args),
  convertFileSrc: (path: string) => path,
}))

describe('saveMeetingAudio', () => {
  beforeEach(() => {
    save.mockReset()
    invoke.mockReset()
  })

  it('offers the kind of file the sound is kept as', async () => {
    const { saveMeetingAudio } = await import('../tauri')
    save.mockResolvedValue('/chosen/place.flac')
    expect(await saveMeetingAudio('m-1', 'Meeting recording.wav', '/data/m-1.flac')).toBe(true)
    expect(save).toHaveBeenCalledWith({
      defaultPath: 'Meeting recording.flac',
      filters: [{ name: 'FLAC', extensions: ['flac'] }],
    })
    expect(invoke).toHaveBeenCalledWith('meeting_audio_save', {
      id: 'm-1',
      path: '/chosen/place.flac',
    })

    save.mockResolvedValue(null)
    expect(await saveMeetingAudio('m-1', 'Meeting recording.wav', '/data/m-1.wav')).toBe(false)
    expect(save).toHaveBeenLastCalledWith({
      defaultPath: 'Meeting recording.wav',
      filters: [{ name: 'WAV', extensions: ['wav'] }],
    })
  })
})
