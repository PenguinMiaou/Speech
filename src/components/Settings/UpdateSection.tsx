import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useAppStore } from '../../stores/appStore'
import { formatBytes } from '../../lib/format-bytes'
import { updateCheck, updateInstall } from '../../lib/tauri'
import { autoCheckEnabled, setAutoCheckEnabled } from '../../lib/update'
import { Toggle } from './shared/Toggle'

const BUTTON =
  'rounded-[10px] border border-border bg-bg-secondary px-3 py-2 text-[13px] text-text-primary transition-colors hover:bg-bg-tertiary disabled:cursor-not-allowed disabled:opacity-50'

/** Asks for a newer release and installs it: only changed files where that is possible. */
export function UpdateSection() {
  const { t } = useTranslation()
  const status = useAppStore((s) => s.updateStatus)
  const setStatus = useAppStore((s) => s.setUpdateStatus)
  const [busy, setBusy] = useState<'checking' | 'installing' | null>(null)
  const [failed, setFailed] = useState(false)
  const [auto, setAuto] = useState(autoCheckEnabled)

  const check = () => {
    setBusy('checking')
    setFailed(false)
    updateCheck()
      .then(setStatus)
      .catch(() => setFailed(true))
      .finally(() => setBusy(null))
  }

  // A successful update restarts the app, so only a failure comes back here.
  const install = () => {
    setBusy('installing')
    setFailed(false)
    updateInstall().catch(() => {
      setFailed(true)
      setBusy(null)
    })
  }

  return (
    <div className="space-y-3 p-3">
      <div className="flex flex-wrap items-center gap-2">
        <button type="button" className={BUTTON} disabled={busy !== null} onClick={check}>
          {t(busy === 'checking' ? 'settings.update.checking' : 'settings.update.check')}
        </button>
        {status?.available && status.patchBytes !== null && (
          <button type="button" className={BUTTON} disabled={busy !== null} onClick={install}>
            {busy === 'installing'
              ? t('settings.update.installing')
              : t('settings.update.install', {
                  version: status.latest,
                  size: formatBytes(status.patchBytes),
                })}
          </button>
        )}
        {status?.available && status.patchBytes === null && (
          <button type="button" className={BUTTON} onClick={() => openUrl(status.releaseUrl)}>
            {t('settings.update.openPage', { version: status.latest })}
          </button>
        )}
      </div>
      {failed ? (
        <p className="text-[12px] text-error">{t('settings.update.failed')}</p>
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
