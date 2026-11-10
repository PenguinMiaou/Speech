import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useAppStore } from '../../stores/appStore'
import { formatBytes } from '../../lib/format-bytes'
import {
  onUpdateProgress,
  updateCheck,
  updateDownload,
  updateRestart,
  type UpdateProgress,
} from '../../lib/tauri'
import { autoCheckEnabled, setAutoCheckEnabled } from '../../lib/update'
import { Toggle } from './shared/Toggle'

const BUTTON =
  'rounded-[10px] border border-border bg-bg-secondary px-3 py-2 text-[13px] text-text-primary transition-colors hover:bg-bg-tertiary disabled:cursor-not-allowed disabled:opacity-50'

/** Asks for a newer release and installs it: only changed files where that is possible. */
export function UpdateSection() {
  const { t } = useTranslation()
  const status = useAppStore((s) => s.updateStatus)
  const setStatus = useAppStore((s) => s.setUpdateStatus)
  const [busy, setBusy] = useState<'checking' | 'downloading' | null>(null)
  const [progress, setProgress] = useState<UpdateProgress | null>(null)
  const [failed, setFailed] = useState(false)
  const [auto, setAuto] = useState(autoCheckEnabled)

  useEffect(() => {
    const unlisten = Promise.resolve(onUpdateProgress(setProgress))
    return () => {
      void unlisten.then((stop) => stop?.())
    }
  }, [])

  const check = () => {
    setBusy('checking')
    setFailed(false)
    updateCheck()
      .then(setStatus)
      .catch(() => setFailed(true))
      .finally(() => setBusy(null))
  }

  // The new files are in place after this; the running app is the old one until it restarts.
  const download = () => {
    setBusy('downloading')
    setProgress(null)
    setFailed(false)
    updateDownload()
      .then(updateCheck)
      .then(setStatus)
      .catch(() => setFailed(true))
      .finally(() => setBusy(null))
  }

  const canPatch = status?.available && status.patchBytes !== null
  // Nothing left to download: the new version is on disk and waits for a restart.
  const ready = canPatch && status.patchBytes === 0
  const percent =
    progress && progress.total > 0 ? Math.round((progress.downloaded / progress.total) * 100) : 0

  return (
    <div className="space-y-3 p-3">
      <div className="flex flex-wrap items-center gap-2">
        {!ready && (
          <button type="button" className={BUTTON} disabled={busy !== null} onClick={check}>
            {t(busy === 'checking' ? 'settings.update.checking' : 'settings.update.check')}
          </button>
        )}
        {canPatch && !ready && (
          <button type="button" className={BUTTON} disabled={busy !== null} onClick={download}>
            {busy === 'downloading'
              ? t('settings.update.installing')
              : t('settings.update.install', {
                  version: status.latest,
                  size: formatBytes(status.patchBytes ?? 0),
                })}
          </button>
        )}
        {ready && (
          <button type="button" className={BUTTON} onClick={() => void updateRestart()}>
            {t('settings.update.restart')}
          </button>
        )}
        {status?.available && status.patchBytes === null && (
          <button type="button" className={BUTTON} onClick={() => openUrl(status.releaseUrl)}>
            {t('settings.update.openPage', { version: status.latest })}
          </button>
        )}
      </div>
      {busy === 'downloading' && progress && progress.total > 0 && (
        <div className="space-y-1">
          <div
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={percent}
            className="h-1 overflow-hidden rounded-full bg-border"
          >
            <div className="h-full bg-accent transition-all" style={{ width: `${percent}%` }} />
          </div>
          <p className="text-[12px] text-text-tertiary">
            {formatBytes(progress.downloaded)} / {formatBytes(progress.total)}
          </p>
        </div>
      )}
      {failed ? (
        <p className="text-[12px] text-error">{t('settings.update.failed')}</p>
      ) : ready ? (
        <p className="text-[12px] text-text-secondary">
          {t('settings.update.ready', { version: status.latest })}
        </p>
      ) : status && !status.available ? (
        <p className="text-[12px] text-text-tertiary">
          {t('settings.update.upToDate', { version: status.current })}
        </p>
      ) : null}
      <Toggle
        checked={auto}
        onChange={(enabled) => {
          setAuto(enabled)
          setAutoCheckEnabled(enabled)
        }}
        label={t('settings.update.auto')}
      />
    </div>
  )
}
