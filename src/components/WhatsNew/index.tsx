import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Sparkles, X } from 'lucide-react'
import { version as currentVersion } from '../../../package.json'
import { WHATS_NEW, versionsToShow } from '../../lib/whatsNew'

const SEEN_KEY = 'whats_new_seen'

function readSeen(): string | null {
  try {
    return localStorage.getItem(SEEN_KEY)
  } catch {
    return null
  }
}

/** Records that the notes up to this version were shown, so they are not shown again. */
export function markWhatsNewSeen(version = currentVersion) {
  try {
    localStorage.setItem(SEEN_KEY, version)
  } catch {
    // Without storage the notes may be shown again; nothing else depends on it.
  }
}

/** What the version just updated to changed, shown once after an update until closed. */
export function WhatsNew() {
  const { t, i18n } = useTranslation()
  const [versions, setVersions] = useState(() => versionsToShow(currentVersion, readSeen()))

  if (versions.length === 0) return null
  const chinese = (i18n.language || '').startsWith('zh')
  const close = () => {
    markWhatsNewSeen()
    setVersions([])
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/25 px-5">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="whats-new-title"
        className="relative w-full max-w-[460px] rounded-[12px] border border-border bg-bg-primary shadow-float"
      >
        <div className="flex items-center gap-2.5 border-b border-border px-5 py-3.5">
          <span className="grid h-8 w-8 place-items-center rounded-full bg-accent/10 text-accent">
            <Sparkles size={16} />
          </span>
          <h2 id="whats-new-title" className="flex-1 text-[15px] font-semibold text-text-primary">
            {t('whatsNew.title', { version: currentVersion })}
          </h2>
          <button
            type="button"
            aria-label={t('common.close')}
            onClick={close}
            className="grid h-7 w-7 place-items-center rounded-[8px] border-none bg-transparent text-text-tertiary hover:bg-bg-hover hover:text-text-primary"
          >
            <X size={16} />
          </button>
        </div>
        <div className="max-h-[60vh] space-y-4 overflow-y-auto px-5 py-4">
          {versions.map((version) => {
            const notes = WHATS_NEW[version]
            return (
              <section key={version} aria-label={version}>
                {versions.length > 1 && (
                  <h3 className="mb-1.5 text-[12px] font-medium text-text-tertiary">{version}</h3>
                )}
                <ul className="list-disc space-y-1.5 pl-5 text-[13px] leading-relaxed text-text-secondary">
                  {(chinese ? notes.zh : notes.en).map((line) => (
                    <li key={line}>{line}</li>
                  ))}
                </ul>
              </section>
            )
          })}
        </div>
      </div>
    </div>
  )
}
