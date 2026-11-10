import { beforeEach, describe, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { useAppStore, type AppConfig } from '../../stores/appStore'
import { saveOnboardingConfig, setCredential, updateConfig } from '../tauri'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const staleConfig: AppConfig = {
  ...useAppStore.getState().config,
  stt_provider: 'groq-whisper',
  stt_api_key: 'old-stt',
  stt_custom_api_key: 'old-custom',
  llm_provider: 'gemini',
  llm_api_key: 'old-llm',
}

describe('provider key persistence across a settings save', () => {
  const vault = new Map<string, string>()
  let nativeConfig: AppConfig
  beforeEach(() => {
    vault.clear()
    vi.mocked(invoke)
      .mockReset()
      .mockImplementation(async (command, args) => {
        const input = args as {
          namespace: string
          provider: string
          value: string
          config: AppConfig
        }
        if (command === 'set_credential')
          vault.set(`${input.namespace}.${input.provider}`, input.value)
        if (command === 'update_config') {
          // Model the original backend migration, which imported every nonempty key.
          const c = input.config
          nativeConfig = c
          if (c.stt_api_key) vault.set(`stt.${c.stt_provider}`, c.stt_api_key)
          if (c.stt_custom_api_key) vault.set('stt.custom-whisper', c.stt_custom_api_key)
          if (c.llm_api_key) vault.set(`llm.${c.llm_provider}`, c.llm_api_key)
        }
        if (command === 'get_config') return nativeConfig as never
        return undefined as never
      })
  })

  it('keeps rotated keys after repeated unrelated saves with a stale frontend snapshot', async () => {
    const saved = await saveOnboardingConfig(staleConfig)
    expect(saved.stt_api_key).toBe('')
    expect(saved.stt_custom_api_key).toBe('')
    expect(saved.llm_api_key).toBe('')
    expect(staleConfig.llm_api_key).toBe('old-llm')
    await setCredential('stt', 'groq-whisper', 'new-stt')
    await setCredential('stt', 'custom-whisper', 'new-custom')
    await setCredential('llm', 'gemini', 'new-llm')
    await updateConfig({ ...staleConfig, theme: 'dark' })
    await updateConfig({ ...staleConfig, polish_style: 'structured' })
    expect(vault.get('stt.groq-whisper')).toBe('new-stt')
    expect(vault.get('stt.custom-whisper')).toBe('new-custom')
    expect(vault.get('llm.gemini')).toBe('new-llm')
  })

  it('does not persist a config or report success when the vault rejects an onboarding key', async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error('vault unavailable'))
    await expect(saveOnboardingConfig(staleConfig)).rejects.toThrow('vault unavailable')
    expect(vi.mocked(invoke).mock.calls.some(([command]) => command === 'update_config')).toBe(
      false,
    )
  })

  it('returns backend recording limits after onboarding instead of the stale draft', async () => {
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === 'get_config'
        ? ({
            ...staleConfig,
            stt_api_key: '',
            stt_custom_api_key: '',
            llm_api_key: '',
            max_recording_seconds: 600,
          } as never)
        : (undefined as never),
    )
    const saved = await saveOnboardingConfig({ ...staleConfig, max_recording_seconds: 30 })
    expect(saved.max_recording_seconds).toBe(600)
  })

  it('does not carry an inactive regular STT key into a keyless custom endpoint', async () => {
    await saveOnboardingConfig({
      ...staleConfig,
      stt_provider: 'custom-whisper',
      stt_custom_api_key: '',
      llm_api_key: '',
    })
    expect(vault.size).toBe(0)
  })

  it('saves the selected custom key without writing the inactive regular-provider key', async () => {
    await saveOnboardingConfig({ ...staleConfig, stt_provider: 'custom-whisper', llm_api_key: '' })
    expect([...vault.entries()]).toEqual([['stt.custom-whisper', 'old-custom']])
  })
})
