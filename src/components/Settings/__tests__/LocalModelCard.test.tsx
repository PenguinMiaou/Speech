import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, screen, fireEvent, waitFor, cleanup, act } from '@testing-library/react'
import { LocalModelCard } from '../LocalModelCard'
import * as tauri from '../../../lib/tauri'
import { useAppStore } from '../../../stores/appStore'
import { capsuleErrorKeyFromPayload } from '../../../lib/capsuleError'
import { ONBOARDING_STT_PROVIDERS } from '../../../lib/constants'

vi.mock('../../../lib/tauri')
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

const LARGE = 'qwen3-asr-1.7b-q8'
const SMALL = 'qwen3-asr-0.6b-q8'
const MOSS = 'moss-transcribe-diarize-q8'

function model(
  id: string,
  overrides: Partial<tauri.LocalModelStatus> = {},
): tauri.LocalModelStatus {
  return {
    id,
    size_bytes: id === LARGE ? 2185030624 : 850423456,
    recommended: id === LARGE,
    installed: false,
    installing: false,
    partial_bytes: 0,
    ...overrides,
  }
}

function event(
  phase: tauri.LocalSttProgress['phase'],
  overrides: Partial<tauri.LocalSttProgress> = {},
) {
  return {
    model_id: LARGE,
    phase,
    downloaded: 0,
    total: 2185030624,
    bytes_per_sec: 0,
    error_code: null,
    ...overrides,
  } as tauri.LocalSttProgress
}

describe('LocalModelCard', () => {
  let emit: (progress: tauri.LocalSttProgress) => void

  beforeEach(() => {
    vi.mocked(tauri.listLocalModels).mockResolvedValue([model(LARGE), model(SMALL)])
    vi.mocked(tauri.installLocalModel).mockResolvedValue(undefined)
    vi.mocked(tauri.deleteLocalModel).mockResolvedValue(undefined)
    vi.mocked(tauri.updateConfig).mockResolvedValue(undefined)
    useAppStore.setState({ savedConfig: null })
    vi.mocked(tauri.onLocalSttProgress).mockImplementation(async (handler) => {
      emit = handler
      return () => {}
    })
    useAppStore.getState().updateConfig({ stt_provider: 'groq-whisper', stt_local_model: LARGE })
  })

  afterEach(() => {
    cleanup()
    vi.clearAllMocks()
  })

  it('shows_install_button_when_missing', async () => {
    render(<LocalModelCard />)
    const buttons = await screen.findAllByRole('button', { name: 'localStt.install' })
    expect(buttons).toHaveLength(2)
    fireEvent.click(buttons[0])
    expect(tauri.installLocalModel).toHaveBeenCalledWith(LARGE)
  })

  it('offers_resume_when_a_partial_download_exists', async () => {
    vi.mocked(tauri.listLocalModels).mockResolvedValue([
      model(LARGE, { partial_bytes: 1000 }),
      model(SMALL),
    ])
    render(<LocalModelCard />)
    expect(await screen.findByRole('button', { name: 'localStt.resume' })).toBeTruthy()
  })

  it('shows_progress_on_event', async () => {
    render(<LocalModelCard />)
    await screen.findAllByRole('button', { name: 'localStt.install' })
    act(() => emit(event('downloading', { downloaded: 1092515312, bytes_per_sec: 10485760 })))
    expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('50')
    fireEvent.click(screen.getByRole('button', { name: 'localStt.cancel' }))
    expect(tauri.cancelLocalInstall).toHaveBeenCalledWith(LARGE)
  })

  it('shows_in_use_for_the_selected_installed_model', async () => {
    useAppStore.getState().updateConfig({ stt_provider: 'local-qwen3', stt_local_model: LARGE })
    vi.mocked(tauri.listLocalModels).mockResolvedValue([
      model(LARGE, { installed: true }),
      model(SMALL),
    ])
    render(<LocalModelCard />)
    expect(await screen.findByText('localStt.active')).toBeTruthy()
  })

  it('a_failed_smoke_test_stays_visible_when_the_file_is_in_place', async () => {
    vi.mocked(tauri.listLocalModels).mockResolvedValue([
      model(LARGE, { installed: true }),
      model(SMALL),
    ])
    render(<LocalModelCard />)
    await screen.findByRole('button', { name: 'localStt.enable' })
    act(() => emit(event('failed', { error_code: 'engine' })))
    expect(screen.getByText('localStt.errorEngine')).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'localStt.enable' })).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'localStt.retry' }))
    expect(tauri.installLocalModel).toHaveBeenCalledWith(LARGE)
  })

  it('names the cause of a failed check and shows what the program reported', async () => {
    vi.mocked(tauri.listLocalModels).mockResolvedValue([model(LARGE, { installed: true })])
    render(<LocalModelCard />)
    await screen.findByRole('button', { name: 'localStt.enable' })
    act(() =>
      emit(
        event('failed', {
          error_code: 'engine_cpu',
          error_detail: 'local speech worker stopped: worker exited (exit code: 0xc000001d)',
        }),
      ),
    )
    expect(screen.getByText('localStt.errorEngineCpu')).toBeTruthy()
    expect(screen.getByText(/0xc000001d/)).toBeTruthy()
  })

  it('install_command_rejection_shows_retry', async () => {
    vi.mocked(tauri.installLocalModel).mockRejectedValue(new Error('unknown model'))
    render(<LocalModelCard />)
    const buttons = await screen.findAllByRole('button', { name: 'localStt.install' })
    fireEvent.click(buttons[0])
    expect(await screen.findByRole('button', { name: 'localStt.retry' })).toBeTruthy()
    expect(screen.getByText('localStt.errorUnknown')).toBeTruthy()
  })

  it('shows_retry_on_failed', async () => {
    render(<LocalModelCard />)
    await screen.findAllByRole('button', { name: 'localStt.install' })
    act(() => emit(event('failed', { error_code: 'disk_full' })))
    expect(screen.getByText('localStt.errorDiskFull')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'localStt.retry' }))
    expect(tauri.installLocalModel).toHaveBeenCalledWith(LARGE)
  })

  it('enables_an_installed_model_with_one_click', async () => {
    vi.mocked(tauri.listLocalModels).mockResolvedValue([
      model(LARGE),
      model(SMALL, { installed: true }),
    ])
    render(<LocalModelCard />)
    fireEvent.click(await screen.findByRole('button', { name: 'localStt.enable' }))
    expect(tauri.installLocalModel).toHaveBeenCalledWith(SMALL)
    expect(useAppStore.getState().config.stt_provider).toBe('groq-whisper')
    expect(await screen.findByText('localStt.testing')).toBeTruthy()
  })

  it('does_not_change_settings_by_itself', async () => {
    render(<LocalModelCard />)
    await screen.findAllByRole('button', { name: 'localStt.install' })
    act(() => emit(event('ready')))
    await act(async () => {})
    expect(tauri.updateConfig).not.toHaveBeenCalled()
  })

  it.each([
    ['STT connection failed: Configuration error: local_model_not_installed'],
    [{ code: 'stt_local_model_missing' }],
  ])('missing_model_has_its_own_capsule_message %#', (payload) => {
    expect(capsuleErrorKeyFromPayload(payload)).toBe('stt_local_model_missing')
  })

  it('explains_each_model_and_marks_the_recommended_one', async () => {
    render(<LocalModelCard />)
    expect(await screen.findByText('Qwen3 1.7B')).toBeTruthy()
    expect(screen.getByText('localStt.recommended')).toBeTruthy()
    expect(screen.getByText('localStt.notes.qwenLarge')).toBeTruthy()
    expect(screen.getByText('localStt.notes.qwenSmall')).toBeTruthy()
  })

  it('folds_the_other_models_until_asked', async () => {
    vi.mocked(tauri.listLocalModels).mockResolvedValue([model(LARGE), model(SMALL), model(MOSS)])
    render(<LocalModelCard />)
    await screen.findByText('Qwen3 1.7B')
    expect(screen.queryByText('MOSS Transcribe')).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /localStt.moreModels/ }))
    expect(screen.getByText('MOSS Transcribe')).toBeTruthy()
    expect(screen.getByText('localStt.notes.moss')).toBeTruthy()
  })

  it('keeps_an_installed_model_visible_even_when_it_is_not_a_main_one', async () => {
    vi.mocked(tauri.listLocalModels).mockResolvedValue([
      model(LARGE),
      model(SMALL),
      model(MOSS, { installed: true }),
    ])
    render(<LocalModelCard />)
    expect(await screen.findByText('MOSS Transcribe')).toBeTruthy()
    expect(screen.queryByRole('button', { name: /localStt.moreModels/ })).toBeNull()
  })

  it('onboarding_does_not_offer_a_model_it_cannot_install', () => {
    expect(ONBOARDING_STT_PROVIDERS.map((provider) => provider.value)).not.toContain('local-qwen3')
  })

  it('delete_needs_a_second_click', async () => {
    useAppStore.getState().updateConfig({ stt_provider: 'local-qwen3', stt_local_model: LARGE })
    vi.mocked(tauri.listLocalModels).mockResolvedValue([
      model(LARGE, { installed: true }),
      model(SMALL),
    ])
    render(<LocalModelCard />)
    fireEvent.click(await screen.findByRole('button', { name: 'localStt.delete' }))
    expect(tauri.deleteLocalModel).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: 'localStt.confirmDelete' }))
    await waitFor(() => expect(tauri.deleteLocalModel).toHaveBeenCalledWith(LARGE))
  })

  it('shows_not_installed_when_selected_model_is_gone', async () => {
    useAppStore.getState().updateConfig({ stt_provider: 'local-qwen3', stt_local_model: LARGE })
    render(<LocalModelCard />)
    expect(await screen.findByText('localStt.notInstalled')).toBeTruthy()
  })

  it('restores_the_download_state_when_reopened_during_install', async () => {
    vi.mocked(tauri.listLocalModels).mockResolvedValue([
      model(LARGE, { installing: true, partial_bytes: 1000 }),
      model(SMALL),
    ])
    render(<LocalModelCard />)
    expect(await screen.findByRole('button', { name: 'localStt.cancel' })).toBeTruthy()
  })
})
