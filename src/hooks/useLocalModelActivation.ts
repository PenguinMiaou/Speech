import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from '../components/toast-service'
import { LOCAL_QWEN3_PROVIDER } from '../lib/constants'
import { onLocalSttProgress, updateConfig as persistConfig } from '../lib/tauri'
import { useAppStore } from '../stores/appStore'

/**
 * Switches dictation to a local model once its install has passed the smoke test. Lives at app
 * level so it still happens after the user leaves the settings page during a long download.
 */
export function useLocalModelActivation() {
  const { t } = useTranslation()

  useEffect(() => {
    // Saved on top of the last saved config, so other unsaved edits in settings stay drafts.
    const activate = async (modelId: string) => {
      const store = useAppStore.getState()
      const previousConfig = store.config
      const patch = { stt_provider: LOCAL_QWEN3_PROVIDER, stt_local_model: modelId }
      const persisted = { ...(store.savedConfig ?? store.config), ...patch }
      store.setConfig({ ...previousConfig, ...patch })
      try {
        await persistConfig(persisted)
        if (store.savedConfig) store.applyPersistedConfigPatch(patch)
        else store.setSavedConfig(persisted)
      } catch {
        store.setConfig(previousConfig)
        toast.error(t('localStt.errorSave'))
      }
    }

    const unlisten = Promise.resolve(
      onLocalSttProgress((event) => {
        if (event.phase === 'ready') void activate(event.model_id)
      }),
    )
    return () => {
      void unlisten.then((stop) => stop?.())
    }
  }, [t])
}
