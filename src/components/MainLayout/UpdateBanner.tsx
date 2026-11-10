import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ArrowUpCircle, X } from 'lucide-react'
import { useAppStore } from '../../stores/appStore'

/** Says a newer version exists and leads to the About page, where it is installed. */
export function UpdateBanner() {
  const { t } = useTranslation()
  const status = useAppStore((s) => s.updateStatus)
  // Closing hides the bar for that version only; a later one shows it again.
  const [closedFor, setClosedFor] = useState<string | null>(null)

  if (!status?.available || closedFor === status.latest) return null

  return (
    <div className="flex items-center gap-2 border-b border-accent/20 bg-accent/10 px-4 py-2">
      <ArrowUpCircle size={14} className="shrink-0 text-accent" />
      <span className="flex-1 text-[12px] text-text-primary">
        {t('settings.update.banner', { version: status.latest })}
      </span>
      <button
        type="button"
        onClick={() => {
          window.location.hash = '#/settings?pane=about'
        }}
        className="shrink-0 cursor-pointer rounded-full border-none bg-accent px-3 py-1 text-[11px] font-medium text-white transition-colors hover:bg-accent-hover"
      >
        {t('settings.update.bannerAction')}
      </button>
      <button
        type="button"
        onClick={() => setClosedFor(status.latest)}
        aria-label={t('common.close')}
        title={t('common.close')}
        className="grid h-6 w-6 shrink-0 place-items-center rounded-[6px] border-none bg-transparent text-text-tertiary hover:bg-bg-hover hover:text-text-secondary"
      >
        <X size={13} />
      </button>
    </div>
  )
}
