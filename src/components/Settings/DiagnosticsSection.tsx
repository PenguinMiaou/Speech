import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { getSystemDiagnostics, type DiagnosticRow } from '../../lib/tauri'

const PART: Record<string, string> = {
  microphone: 'settings.diagnosticMicrophone',
  accessibility: 'settings.diagnosticAccessibility',
  hotkey: 'settings.diagnosticHotkey',
  clipboard: 'settings.diagnosticClipboard',
  insertion: 'settings.diagnosticInsertion',
  platform: 'settings.diagnosticPlatform',
}

const STATE: Record<string, [label: string, colour: string]> = {
  ok: ['settings.diagnosticStatusOk', 'text-text-tertiary'],
  warning: ['settings.diagnosticStatusWarning', 'text-amber-500'],
  error: ['settings.diagnosticStatusError', 'text-error'],
  notApplicable: ['settings.diagnosticStatusNotApplicable', 'text-text-tertiary'],
  checking: ['settings.diagnosticStatusChecking', 'text-text-tertiary'],
}

/** Checks the microphone, permissions, hotkey and text output on request. */
export function DiagnosticsSection() {
  const { t } = useTranslation()
  const [rows, setRows] = useState<DiagnosticRow[] | null>(null)
  const [busy, setBusy] = useState(false)
  const [failed, setFailed] = useState(false)

  const check = () => {
    setBusy(true)
    setFailed(false)
    Promise.resolve(getSystemDiagnostics())
      .then((report) => setRows(report?.rows ?? []))
      .catch(() => setFailed(true))
      .finally(() => setBusy(false))
  }

  return (
    <div className="space-y-3 p-3">
      <button
        type="button"
        onClick={check}
        disabled={busy}
        className="rounded-[8px] border border-border bg-bg-secondary px-3 py-2 text-[13px] text-text-primary transition-colors hover:bg-bg-tertiary disabled:cursor-not-allowed disabled:opacity-50"
      >
        {t(busy ? 'settings.diagnosticStatusChecking' : 'settings.refreshDiagnostics')}
      </button>
      {failed && <p className="text-[12px] text-error">{t('settings.diagnosticsUnavailable')}</p>}
      {rows && rows.length > 0 && (
        <ul className="divide-y divide-border rounded-[8px] border border-border">
          {rows.map((row) => {
            const [label, colour] = STATE[row.status] ?? STATE.checking
            const fine = row.status === 'ok' || row.status === 'notApplicable'
            return (
              <li key={row.id} className="px-3 py-2 text-[13px]">
                <div className="flex items-center justify-between gap-3">
                  <span className="text-text-primary">
                    {PART[row.id] ? t(PART[row.id]) : row.id}
                  </span>
                  <span className={colour}>{t(label)}</span>
                </div>
                {!fine && row.message && (
                  <p className="mt-1 break-words text-[12px] text-text-secondary">{row.message}</p>
                )}
              </li>
            )
          })}
        </ul>
      )}
    </div>
  )
}
