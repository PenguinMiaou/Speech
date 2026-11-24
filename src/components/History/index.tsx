import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { useTranslation } from 'react-i18next'
import { Search, Trash2, MoreHorizontal } from 'lucide-react'
import { spring } from '../../lib/animations'
import { useAppStore, type HistoryEntry } from '../../stores/appStore'
import {
  addCorrectionRule,
  clearHistory,
  copyText,
  getCorrectionRules,
  getHistory,
} from '../../lib/tauri'
import { toast } from '../toast-service'
import { AppContextMeta } from './AppContextMeta'
import { openContextMenu } from '../ContextMenu'
import { CopyButton } from '../CopyButton'
import { CreateCorrectionDialog } from './CreateCorrectionDialog'

/** The day `date` falls on here, as YYYY-MM-DD. */
function localDay(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/** How many entries one page of history holds, the same count the app loads at start. */
const PAGE_SIZE = 200

/** A local day kept as YYYY-MM-DD, written out in the interface language. */
function formatDay(day: string, language: string): string {
  const [year, month, date] = day.split('-').map(Number)
  if (!year || !month || !date) return day
  const when = new Date(year, month - 1, date)
  try {
    return new Intl.DateTimeFormat(language, {
      weekday: 'short',
      month: 'long',
      day: 'numeric',
      ...(year === new Date().getFullYear() ? {} : { year: 'numeric' }),
    }).format(when)
  } catch {
    return day
  }
}

/** The text with every place that matches the search marked, ignoring case. */
function Highlighted({ text, needle }: { text: string; needle: string }) {
  if (!needle) return <>{text}</>
  const lower = text.toLowerCase()
  // Lowercasing can change the length of some letters; then the places would not line up.
  if (lower.length !== text.length) return <>{text}</>
  const parts: React.ReactNode[] = []
  let from = 0
  let at = lower.indexOf(needle)
  while (at !== -1) {
    if (at > from) parts.push(text.slice(from, at))
    parts.push(
      <mark key={at} className="rounded-xs bg-accent/20 px-[1px] text-text-primary">
        {text.slice(at, at + needle.length)}
      </mark>,
    )
    from = at + needle.length
    at = lower.indexOf(needle, from)
  }
  if (from < text.length) parts.push(text.slice(from))
  return <>{parts}</>
}

export function History() {
  const history = useAppStore((s) => s.history)
  const setHistory = useAppStore((s) => s.setHistory)
  const setCorrectionRules = useAppStore((s) => s.setCorrectionRules)
  const { t, i18n } = useTranslation()
  const [search, setSearch] = useState('')
  const [appFilter, setAppFilter] = useState('')
  const [loadingMore, setLoadingMore] = useState(false)
  /** How many entries were shown when a page came back short, so there is nothing older. */
  const [endAt, setEndAt] = useState<number | null>(null)
  const [menuEntryId, setMenuEntryId] = useState<number | null>(null)
  const [correctionEntry, setCorrectionEntry] = useState<HistoryEntry | null>(null)
  const [confirmingClear, setConfirmingClear] = useState(false)
  const menuTriggerEntryId = useRef<number | null>(null)

  const closeEntryMenu = useCallback(() => {
    setMenuEntryId(null)
    const entryId = menuTriggerEntryId.current
    window.setTimeout(() => {
      if (entryId === null) return
      document.querySelector<HTMLButtonElement>(`[data-history-menu-trigger="${entryId}"]`)?.focus()
    }, 0)
  }, [])

  useEffect(() => {
    if (menuEntryId === null) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      closeEntryMenu()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [closeEntryMenu, menuEntryId])

  const needle = search.trim().toLowerCase()

  // The apps the loaded entries were dictated into, the most used first.
  const apps = useMemo(() => {
    const counts = new Map<string, number>()
    for (const entry of history) {
      if (!entry.context_label) continue
      counts.set(entry.context_label, (counts.get(entry.context_label) ?? 0) + 1)
    }
    return Array.from(counts.entries())
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .map(([label]) => label)
  }, [history])

  const filtered = useMemo(
    () =>
      history.filter(
        (h) =>
          (!appFilter || h.context_label === appFilter) &&
          (!needle ||
            h.polished_text.toLowerCase().includes(needle) ||
            h.raw_text.toLowerCase().includes(needle) ||
            h.context_label.toLowerCase().includes(needle)),
      ),
    [history, needle, appFilter],
  )

  const filtering = needle !== '' || appFilter !== ''
  // A refresh after each dictation puts the list back to the first page; then the older
  // entries can be loaded again.
  const canLoadMore = history.length >= PAGE_SIZE && (endAt === null || history.length < endAt)

  // Fetches the next older page and adds the entries not already shown.
  const handleLoadMore = async () => {
    if (loadingMore) return
    setLoadingMore(true)
    try {
      const current = useAppStore.getState().history
      const page = await getHistory(PAGE_SIZE, current.length)
      const known = new Set(current.map((entry) => entry.id))
      const next = [...current, ...page.filter((entry) => !known.has(entry.id))]
      setHistory(next)
      setEndAt(page.length < PAGE_SIZE ? next.length : null)
    } catch (e) {
      console.error('Failed to load more history:', e)
      toast.error(t('history.failedToLoadMore'))
    } finally {
      setLoadingMore(false)
    }
  }

  /** Copies an entry from its right-click menu, saying so as the copy button would. */
  const handleCopy = (text: string) => {
    void Promise.resolve(copyText(text))
      .then(() => toast.success(t('common.copied')))
      .catch(() => toast.error(t('history.failedToCopy')))
  }

  const handleClear = async () => {
    try {
      await clearHistory()
      setHistory([])
      setEndAt(null)
      setConfirmingClear(false)
    } catch (e) {
      console.error('Failed to clear history:', e)
      toast.error(t('history.failedToClear'))
    }
  }

  const handleCreateCorrection = async (pattern: string, replacement: string) => {
    try {
      await addCorrectionRule(pattern, replacement)
      setCorrectionRules(await getCorrectionRules())
      setCorrectionEntry(null)
      toast.success(t('history.correctionCreated'))
    } catch (error) {
      console.error('Failed to create correction from history:', error)
      toast.error(t('history.failedToCreateCorrection'))
      throw error
    }
  }

  const outputStatusLabel = (status: string | null) => {
    switch (status) {
      case 'partial':
        return t('history.outputStatus.partial')
      case 'fallback':
        return t('history.outputStatus.fallback')
      case 'clipboard_fallback':
        return t('history.outputStatus.clipboardFallback')
      default:
        return null
    }
  }

  // Group by date. Entries carry the local time they were made at, so the days are local too.
  const grouped = useMemo(() => {
    const map = new Map<string, typeof filtered>()
    const today = localDay(new Date())
    const yesterday = localDay(new Date(Date.now() - 86400000))
    for (const entry of filtered) {
      const date = entry.created_at.split('T')[0] || entry.created_at.split(' ')[0]
      const label =
        date === today
          ? t('history.today')
          : date === yesterday
            ? t('history.yesterday')
            : formatDay(date, i18n.language)
      if (!map.has(label)) map.set(label, [])
      map.get(label)!.push(entry)
    }
    return map
  }, [filtered, t, i18n.language])

  return (
    <div className="mx-auto flex h-full w-full max-w-[820px] flex-col bg-bg-primary text-text-primary">
      {/* Header */}
      <div className="page-header">
        <h2 className="page-title">{t('history.title')}</h2>
      </div>

      {/* Search and app filter */}
      <div className="px-8 py-3 flex items-center gap-2">
        <div className="relative flex-1 min-w-0">
          <Search
            size={14}
            className="absolute left-3 top-1/2 -translate-y-1/2 text-text-tertiary"
          />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            data-find
            placeholder={t('history.searchPlaceholder')}
            aria-label={t('history.searchPlaceholder')}
            className="w-full pl-8 pr-3 py-2.5 bg-bg-secondary border border-border rounded-md text-[13px] text-text-primary outline-none focus:ring-2 focus:ring-jelly-primary focus:border-jelly-primary transition-all jelly-btn"
            style={{ transform: 'none' }}
          />
        </div>
        {apps.length > 1 && (
          <select
            value={appFilter}
            onChange={(e) => setAppFilter(e.target.value)}
            aria-label={t('history.filterByApp')}
            className="max-w-[180px] shrink-0 truncate py-2.5 pl-3 pr-7 bg-bg-secondary border border-border rounded-md text-[13px] text-text-primary outline-none cursor-pointer focus-visible:ring-2 focus-visible:ring-jelly-primary"
          >
            <option value="">{t('history.allApps')}</option>
            {apps.map((label) => (
              <option key={label} value={label}>
                {label}
              </option>
            ))}
          </select>
        )}
      </div>

      {/* List */}
      <div className="flex-1 overflow-y-auto px-8 pb-4">
        {filtered.length === 0 ? (
          filtering && history.length > 0 ? (
            <div role="status" className="flex flex-col items-center gap-2 py-12 text-center">
              <Search size={18} className="text-text-tertiary" aria-hidden="true" />
              <p className="text-[13px] text-text-secondary">{t('history.noResults')}</p>
              <button
                type="button"
                onClick={() => {
                  setSearch('')
                  setAppFilter('')
                }}
                className="rounded-md border border-border bg-transparent px-3 py-1.5 text-[12px] text-text-secondary cursor-pointer hover:text-text-primary hover:bg-bg-secondary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
              >
                {t('history.clearFilters')}
              </button>
            </div>
          ) : (
            <p className="text-center text-text-tertiary text-[13px] py-12">
              {t('history.noHistory')}
              <br />
              <span className="text-[12px]">{t('history.noHistoryHint')}</span>
            </p>
          )
        ) : (
          <AnimatePresence>
            {Array.from(grouped.entries()).map(([label, entries]) => (
              <div key={label} className="mb-4">
                <h3 className="section-label mb-2 px-1">{label}</h3>
                <div className="space-y-0.5">
                  {entries.map((entry) => (
                    <motion.div
                      key={entry.id}
                      onContextMenu={(event: React.MouseEvent) =>
                        openContextMenu(event, [
                          {
                            label: t('contextMenu.copyAll'),
                            onSelect: () => handleCopy(entry.polished_text),
                          },
                          {
                            label: t('history.createCorrection'),
                            onSelect: () => setCorrectionEntry(entry),
                          },
                        ])
                      }
                      transition={spring.gentle}
                      className="group flex items-start gap-3 px-3 py-2.5 rounded-md hover:bg-bg-secondary transition-colors"
                    >
                      <div className="flex-1 min-w-0">
                        <p className="text-[13px] text-text-primary leading-relaxed">
                          <Highlighted text={entry.polished_text} needle={needle} />
                        </p>
                        <AppContextMeta
                          iconKey={entry.context_icon_key}
                          family={entry.context_family}
                          label={entry.context_label}
                          time={entry.created_at.split('T')[1]?.slice(0, 5) || ''}
                          providerKind={entry.provider_kind}
                          browserAccessStatus={entry.browser_access_status}
                        />
                        {entry.output_status && outputStatusLabel(entry.output_status) && (
                          <p className="text-[11px] text-warning mt-1 leading-snug break-words">
                            {outputStatusLabel(entry.output_status)}
                            {entry.output_error ? `  ${entry.output_error}` : ''}
                          </p>
                        )}
                      </div>
                      <div className="flex flex-shrink-0 items-center">
                        <CopyButton
                          text={entry.polished_text}
                          label={t('history.copyEntry', {
                            text: entry.polished_text.slice(0, 30),
                          })}
                          size={13}
                          className="flex flex-shrink-0 cursor-pointer items-center rounded-md border-none bg-transparent p-1.5 text-text-tertiary opacity-40 transition-opacity hover:bg-bg-tertiary hover:text-accent focus-visible:opacity-100 group-hover:opacity-100"
                        />
                        <div className="relative">
                          <button
                            type="button"
                            onClick={() => {
                              menuTriggerEntryId.current = entry.id
                              setMenuEntryId((current) => (current === entry.id ? null : entry.id))
                            }}
                            data-history-menu-trigger={entry.id}
                            aria-label={t('history.moreActions')}
                            aria-haspopup="menu"
                            aria-expanded={menuEntryId === entry.id}
                            className="opacity-40 group-hover:opacity-100 focus-visible:opacity-100 aria-expanded:opacity-100 p-1.5 rounded-md hover:bg-bg-tertiary transition-opacity duration-150 ease-out bg-transparent border-none cursor-pointer text-text-tertiary hover:text-text-primary flex-shrink-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                          >
                            <MoreHorizontal size={13} />
                          </button>
                          {menuEntryId === entry.id && (
                            <>
                              <div className="fixed inset-0 z-30" onClick={closeEntryMenu} />
                              <div
                                role="menu"
                                className="absolute right-0 top-7 z-40 min-w-40 rounded-md border border-border bg-bg-primary py-1 shadow-float"
                              >
                                <button
                                  type="button"
                                  role="menuitem"
                                  onClick={() => {
                                    setMenuEntryId(null)
                                    setCorrectionEntry(entry)
                                  }}
                                  className="h-8 w-full bg-transparent px-3 text-left text-[12px] text-text-primary hover:bg-bg-secondary"
                                >
                                  {t('history.createCorrection')}
                                </button>
                              </div>
                            </>
                          )}
                        </div>
                      </div>
                    </motion.div>
                  ))}
                </div>
              </div>
            ))}
          </AnimatePresence>
        )}
        {canLoadMore && (
          <div className="flex justify-center pt-1 pb-2">
            <button
              type="button"
              onClick={handleLoadMore}
              disabled={loadingMore}
              className="rounded-md border border-border bg-transparent px-4 py-1.5 text-[12px] text-text-secondary cursor-pointer hover:text-text-primary hover:bg-bg-secondary disabled:opacity-60 disabled:cursor-default focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            >
              {loadingMore ? t('history.loadingMore') : t('history.loadMore')}
            </button>
          </div>
        )}
      </div>

      {/* Clear button — jelly */}
      {history.length > 0 && (
        <div className="space-y-2 border-t border-border px-8 py-3">
          {confirmingClear && (
            <div className="rounded-md border border-error/20 bg-error/10 px-3 py-2">
              <p className="text-[12px] leading-relaxed text-text-secondary">
                {t('history.clearConfirm')}
              </p>
              <div className="mt-2 flex justify-end gap-2">
                <button
                  type="button"
                  onClick={() => setConfirmingClear(false)}
                  className="rounded-md border border-border bg-transparent px-2.5 py-1 text-[11px] text-text-secondary hover:text-text-primary"
                >
                  {t('common.cancel')}
                </button>
                <button
                  type="button"
                  onClick={handleClear}
                  className="rounded-md border border-error/30 bg-error/15 px-2.5 py-1 text-[11px] font-medium text-error hover:bg-error/20"
                >
                  {t('history.confirmClear')}
                </button>
              </div>
            </div>
          )}
          <motion.button
            onClick={() => setConfirmingClear(true)}
            transition={spring.gentle}
            className="flex w-full cursor-pointer items-center justify-center gap-1.5 rounded-md py-2 text-[12px] text-text-tertiary transition-colors hover:text-error jelly-btn"
          >
            <Trash2 size={12} />
            {t('history.clearAll')}
          </motion.button>
        </div>
      )}
      {correctionEntry && (
        <CreateCorrectionDialog
          entry={correctionEntry}
          onCancel={() => {
            setCorrectionEntry(null)
            const entryId = menuTriggerEntryId.current
            window.setTimeout(() => {
              if (entryId === null) return
              document
                .querySelector<HTMLButtonElement>(`[data-history-menu-trigger="${entryId}"]`)
                ?.focus()
            }, 0)
          }}
          onSave={handleCreateCorrection}
        />
      )}
    </div>
  )
}
