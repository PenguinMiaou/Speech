import { useEffect, useId, useRef } from 'react'
import { useTranslation } from 'react-i18next'

interface Props {
  saving: boolean
  onSave: () => void
  onDiscard: () => void
  onStay: () => void
}

/** Asked when the user leaves Settings with edits not yet saved: save them, drop them, or stay. */
export function UnsavedLeaveDialog({ saving, onSave, onDiscard, onStay }: Props) {
  const { t } = useTranslation()
  const saveRef = useRef<HTMLButtonElement>(null)
  const titleId = useId()
  const bodyId = useId()

  useEffect(() => {
    saveRef.current?.focus()
  }, [])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || saving) return
      event.preventDefault()
      onStay()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onStay, saving])

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/25 px-5">
      <div className="fixed inset-0" onClick={saving ? undefined : onStay} />
      <div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={bodyId}
        className="relative z-10 w-full max-w-[400px] rounded-md border border-border bg-bg-primary shadow-float"
      >
        <div className="px-4 pt-4 pb-3">
          <h3 id={titleId} className="text-[14px] font-medium text-text-primary">
            {t('settings.leaveTitle')}
          </h3>
          <p id={bodyId} className="mt-1.5 text-[13px] leading-relaxed text-text-secondary">
            {t('settings.leaveBody')}
          </p>
        </div>
        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <button
            type="button"
            onClick={onStay}
            disabled={saving}
            className="rounded-md border border-border bg-transparent px-3 py-1.5 text-[12px] text-text-secondary hover:text-text-primary disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
          >
            {t('settings.leaveStay')}
          </button>
          <button
            type="button"
            onClick={onDiscard}
            disabled={saving}
            className="rounded-md border border-border bg-transparent px-3 py-1.5 text-[12px] text-error hover:bg-error/10 disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
          >
            {t('settings.leaveDiscard')}
          </button>
          <button
            ref={saveRef}
            type="button"
            onClick={onSave}
            disabled={saving}
            className="rounded-md border-none bg-accent px-3 py-1.5 text-[12px] text-white hover:bg-accent-hover disabled:opacity-70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2"
          >
            {saving ? t('common.saving') : t('settings.leaveSave')}
          </button>
        </div>
      </div>
    </div>
  )
}
