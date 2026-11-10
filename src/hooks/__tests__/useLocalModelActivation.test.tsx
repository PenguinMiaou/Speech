import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { act, cleanup, renderHook, waitFor } from '@testing-library/react'
import { useLocalModelActivation } from '../useLocalModelActivation'
import * as tauri from '../../lib/tauri'
import { toast } from '../../components/toast-service'
import { useAppStore } from '../../stores/appStore'

vi.mock('../../lib/tauri')
vi.mock('../../components/toast-service', () => ({
  toast: Object.assign(vi.fn(), { error: vi.fn() }),
}))
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

const MODEL = 'qwen3-asr-1.7b-q8'

function event(phase: tauri.LocalSttProgress['phase']): tauri.LocalSttProgress {
  return {
    model_id: MODEL,
    phase,
    downloaded: 0,
    total: 1,
    bytes_per_sec: 0,
    error_code: null,
  }
}

describe('useLocalModelActivation', () => {
  let emit: (progress: tauri.LocalSttProgress) => void

  beforeEach(() => {
    vi.mocked(tauri.updateConfig).mockResolvedValue(undefined)
    vi.mocked(tauri.onLocalSttProgress).mockImplementation(async (handler) => {
      emit = handler
      return () => {}
    })
    useAppStore.setState({ savedConfig: null })
    useAppStore.getState().updateConfig({ stt_provider: 'groq-whisper', stt_local_model: MODEL })
  })

  afterEach(() => {
    cleanup()
    vi.clearAllMocks()
  })

  async function listening() {
    renderHook(() => useLocalModelActivation())
    await waitFor(() => expect(tauri.onLocalSttProgress).toHaveBeenCalled())
    await act(async () => {})
  }

  it('saves_the_local_provider_when_an_install_passes_with_settings_closed', async () => {
    await listening()
    act(() => emit(event('ready')))
    await waitFor(() => expect(tauri.updateConfig).toHaveBeenCalledTimes(1))
    const persisted = vi.mocked(tauri.updateConfig).mock.calls[0][0]
    expect(persisted.stt_provider).toBe('local-qwen3')
    expect(persisted.stt_local_model).toBe(MODEL)
    expect(useAppStore.getState().config.stt_provider).toBe('local-qwen3')
    await waitFor(() =>
      expect(useAppStore.getState().savedConfig?.stt_provider).toBe('local-qwen3'),
    )
  })

  it('keeps_other_unsaved_settings_as_drafts', async () => {
    const saved = useAppStore.getState().config
    useAppStore.setState({ savedConfig: saved })
    useAppStore.getState().updateConfig({ polish_custom_prompt: 'draft-not-saved-yet' })
    await listening()
    act(() => emit(event('ready')))
    await waitFor(() => expect(tauri.updateConfig).toHaveBeenCalledTimes(1))
    expect(vi.mocked(tauri.updateConfig).mock.calls[0][0].polish_custom_prompt).toBe(
      saved.polish_custom_prompt,
    )
    expect(useAppStore.getState().config.polish_custom_prompt).toBe('draft-not-saved-yet')
  })

  it.each(['downloading', 'verifying', 'testing', 'failed', 'cancelled'] as const)(
    'does_not_activate_on_%s',
    async (phase) => {
      await listening()
      act(() => emit(event(phase)))
      await act(async () => {})
      expect(tauri.updateConfig).not.toHaveBeenCalled()
      expect(useAppStore.getState().config.stt_provider).toBe('groq-whisper')
    },
  )

  it('a_failed_save_reverts_and_says_so', async () => {
    vi.mocked(tauri.updateConfig).mockRejectedValue(new Error('disk'))
    await listening()
    act(() => emit(event('ready')))
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith('localStt.errorSave'))
    expect(useAppStore.getState().config.stt_provider).toBe('groq-whisper')
  })
})
