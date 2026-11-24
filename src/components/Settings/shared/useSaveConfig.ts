import { useCallback, useState } from 'react'
import { useAppStore } from '../../../stores/appStore'
import {
  getConfig,
  getHotkeyRegistrationError,
  updateConfig,
  setAutoStart,
} from '../../../lib/tauri'
import { toast } from '../../toast-service'

export type SaveResult = 'idle' | 'success' | 'error'

/** Saves the edited settings, or puts the saved ones back. One instance is shared by the
 * save bar, the Ctrl/Cmd+S shortcut and the leave prompt, so they show one state. */
export function useSaveConfig() {
  const setConfig = useAppStore((s) => s.setConfig)
  const resetConfig = useAppStore((s) => s.resetConfig)
  const setSavedConfig = useAppStore((s) => s.setSavedConfig)
  const setHotkeyRegistrationError = useAppStore((s) => s.setHotkeyRegistrationError)
  const [saving, setSaving] = useState(false)
  const [saveResult, setSaveResult] = useState<SaveResult>('idle')
  const [errorMsg, setErrorMsg] = useState('')

  const refreshHotkeyRegistrationError = useCallback(async () => {
    try {
      setHotkeyRegistrationError(await getHotkeyRegistrationError())
    } catch (error) {
      console.error('[settings] failed to refresh hotkey registration error', error)
    }
  }, [setHotkeyRegistrationError])

  /** Saves the current settings; resolves true when they were saved. */
  const save = useCallback(async (): Promise<boolean> => {
    if (saving) return false
    const { config, savedConfig } = useAppStore.getState()
    setSaving(true)
    setSaveResult('idle')
    setErrorMsg('')
    const autoStartChanged = savedConfig !== null && savedConfig.auto_start !== config.auto_start
    let autoStartApplied = false
    try {
      if (autoStartChanged) {
        await setAutoStart(config.auto_start)
        autoStartApplied = true
      }
      await updateConfig(config)
      await refreshHotkeyRegistrationError()
      setSavedConfig(config)
      setSaveResult('success')
      setTimeout(() => {
        setSaveResult('idle')
      }, 1500)
      return true
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Failed to save settings'
      if (autoStartApplied && savedConfig) {
        await setAutoStart(savedConfig.auto_start).catch((rollbackError) => {
          console.error('[settings] failed to roll back auto-start', rollbackError)
        })
      }
      toast(msg, 'error')
      try {
        const backendConfig = await getConfig()
        setConfig(backendConfig)
        setSavedConfig(backendConfig)
      } catch {
        // Keep the dirty state visible if backend truth cannot be reloaded.
      }
      await refreshHotkeyRegistrationError()
      setErrorMsg(msg)
      setSaveResult('error')
      return false
    } finally {
      setSaving(false)
    }
  }, [saving, refreshHotkeyRegistrationError, setConfig, setSavedConfig])

  const reset = useCallback(() => {
    setSaveResult('idle')
    setErrorMsg('')
    resetConfig()
  }, [resetConfig])

  return { save, reset, saving, saveResult, errorMsg }
}

export type ConfigSaver = ReturnType<typeof useSaveConfig>
