import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useAppStore } from '../../stores/appStore'
import { formatBytes } from '../../lib/format-bytes'
import {
  onVoiceIsolationProgress,
  voiceIsolationDeleteSample,
  voiceIsolationEnrollNext,
  voiceIsolationInstall,
  voiceIsolationStatus,
  type VoiceIsolationProgress,
  type VoiceIsolationStatus,
} from '../../lib/tauri'
import { Toggle } from './shared/Toggle'

const BUTTON =
  'rounded-[8px] border border-border bg-bg-secondary px-3 py-2 text-[13px] text-text-primary transition-colors hover:bg-bg-tertiary disabled:cursor-not-allowed disabled:opacity-50'
const NOTE = 'text-[12px] leading-relaxed text-text-tertiary'

/** Keeps only the user's own voice in local recognition: the switch, the model, the sample. */
export function VoiceIsolationSection() {
  const { t } = useTranslation()
  const enabled = useAppStore((s) => s.config.voice_isolation_enabled)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const [status, setStatus] = useState<VoiceIsolationStatus | null>(null)
  const [download, setDownload] = useState<'idle' | 'running' | 'failed'>('idle')
  const [progress, setProgress] = useState<VoiceIsolationProgress | null>(null)

  useEffect(() => {
    const unlisten = Promise.resolve(onVoiceIsolationProgress(setProgress))
    return () => {
      void unlisten.then((stop) => stop?.())
    }
  }, [])

  const refresh = useCallback(() => {
    voiceIsolationStatus()
      .then(setStatus)
      .catch(() => {})
  }, [])

  const install = useCallback(() => {
    setProgress(null)
    setDownload('running')
    voiceIsolationInstall()
      .then(() => setDownload('idle'))
      .catch(() => setDownload('failed'))
      .finally(refresh)
  }, [refresh])

  // A download started before this page was opened keeps running; asking again waits for it,
  // so the page shows its progress and its end.
  const pickedUp = useRef(false)
  useEffect(() => {
    if (!status?.downloading || pickedUp.current) return
    pickedUp.current = true
    install()
  }, [status?.downloading, install])

  // A sample is recorded by dictating elsewhere, so the state is read again on return.
  useEffect(() => {
    refresh()
    window.addEventListener('focus', refresh)
    return () => window.removeEventListener('focus', refresh)
  }, [refresh])

  const record = () => {
    voiceIsolationEnrollNext(true)
      .catch(() => {})
      .finally(refresh)
  }
  const remove = () => {
    voiceIsolationDeleteSample()
      .catch(() => {})
      .finally(refresh)
  }

  return (
    <div className="space-y-3">
      <Toggle
        checked={enabled}
        onChange={(voice_isolation_enabled) => updateConfig({ voice_isolation_enabled })}
        label={t('settings.voiceIsolation.title')}
      />
      {enabled && status && !status.modelInstalled && (
        <div className="space-y-2">
          <button
            type="button"
            className={BUTTON}
            disabled={download === 'running'}
            onClick={install}
          >
            {download === 'running'
              ? t('settings.voiceIsolation.downloading')
              : t('settings.voiceIsolation.download', { size: formatBytes(status.modelBytes) })}
          </button>
          {download === 'running' && progress && progress.total > 0 && (
            <div className="space-y-1">
              <div
                role="progressbar"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={Math.round((progress.downloaded / progress.total) * 100)}
                className="h-1 overflow-hidden rounded-full bg-border"
              >
                <div
                  className="h-full bg-accent transition-all"
                  style={{ width: `${(progress.downloaded / progress.total) * 100}%` }}
                />
              </div>
              <p className="text-[12px] text-text-tertiary">
                {formatBytes(progress.downloaded)} / {formatBytes(progress.total)}
                {progress.bytesPerSec ? ` (${formatBytes(progress.bytesPerSec)}/s)` : ''}
              </p>
            </div>
          )}
          {download === 'failed' && (
            <p className="text-[12px] text-error">{t('settings.voiceIsolation.downloadFailed')}</p>
          )}
        </div>
      )}
      {enabled && status?.modelInstalled && (
        <div className="space-y-2">
          <div className="flex gap-2">
            <button type="button" className={BUTTON} onClick={record}>
              {t(
                status.hasSample
                  ? 'settings.voiceIsolation.recordAgain'
                  : 'settings.voiceIsolation.record',
              )}
            </button>
            {status.hasSample && (
              <button type="button" className={BUTTON} onClick={remove}>
                {t('settings.voiceIsolation.delete')}
              </button>
            )}
          </div>
          {status.enrollmentPending ? (
            <p className={NOTE}>{t('settings.voiceIsolation.pending')}</p>
          ) : status.enrollmentError ? (
            <p className="text-[12px] text-error">
              {t(`settings.voiceIsolation.error.${status.enrollmentError}`)}
            </p>
          ) : !status.hasSample ? (
            <p className={NOTE}>{t('settings.voiceIsolation.needsSample')}</p>
          ) : null}
        </div>
      )}
    </div>
  )
}
