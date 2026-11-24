import { useId, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Search } from 'lucide-react'
import { PANES, type PaneId } from './SettingsSidebar'
import { SETTINGS_SEARCH_INDEX } from './settingsSearchIndex'

const MAX_RESULTS = 8

interface Props {
  /** Opens the pane that holds the chosen setting, and points at the label `target`. */
  onPick: (pane: PaneId, target: string) => void
  /** Called when the search is finished with: a result picked or Escape pressed. */
  onDone?: () => void
  autoFocus?: boolean
}

/** A box that finds a setting in any pane by the words on its label. */
export function SettingsSearch({ onPick, onDone, autoFocus }: Props) {
  const { t } = useTranslation()
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const [focused, setFocused] = useState(false)
  const listId = useId()
  const needle = query.trim().toLowerCase()

  const results = useMemo(() => {
    if (!needle) return []
    const paneLabels = Object.fromEntries(PANES.map((pane) => [pane.id, t(pane.labelKey)]))
    const all = SETTINGS_SEARCH_INDEX.map((entry) => ({
      ...entry,
      label: t(entry.key),
      target: t(entry.inside ?? entry.key),
      paneLabel: paneLabels[entry.pane] ?? entry.pane,
    }))
    // Matches on the setting's own label come before matches on the pane's name.
    const byLabel = all.filter((entry) => entry.label.toLowerCase().includes(needle))
    const byPane = all.filter(
      (entry) =>
        !entry.label.toLowerCase().includes(needle) &&
        entry.paneLabel.toLowerCase().includes(needle),
    )
    return [...byLabel, ...byPane].slice(0, MAX_RESULTS)
  }, [needle, t])

  const pick = (index: number) => {
    const result = results[index]
    if (!result) return
    onPick(result.pane, result.target)
    setQuery('')
    setActive(0)
    onDone?.()
  }

  const onKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'ArrowDown' && results.length) {
      event.preventDefault()
      setActive((index) => (index + 1) % results.length)
    } else if (event.key === 'ArrowUp' && results.length) {
      event.preventDefault()
      setActive((index) => (index - 1 + results.length) % results.length)
    } else if (event.key === 'Enter') {
      event.preventDefault()
      pick(active)
    } else if (event.key === 'Escape') {
      event.preventDefault()
      event.stopPropagation()
      setQuery('')
      setActive(0)
      onDone?.()
    }
  }

  // The list shows while the box has focus; clicking elsewhere closes it.
  const open = focused && needle !== ''
  const optionId = (index: number) => `${listId}-option-${index}`

  return (
    <div className="relative">
      <Search
        size={13}
        aria-hidden="true"
        className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-tertiary"
      />
      <input
        value={query}
        onChange={(event) => {
          setQuery(event.target.value)
          setActive(0)
        }}
        onKeyDown={onKeyDown}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        autoFocus={autoFocus}
        role="combobox"
        data-find
        aria-label={t('settings.searchPlaceholder')}
        aria-expanded={open}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={open && results.length ? optionId(active) : undefined}
        placeholder={t('settings.searchPlaceholder')}
        className="w-full rounded-md border border-border bg-bg-secondary py-1.5 pl-7 pr-2 text-[12px] text-text-primary outline-none focus:border-border-focus focus-visible:ring-2 focus-visible:ring-accent/40"
      />
      {open && (
        <div className="absolute left-0 top-full z-40 mt-1 w-[260px] rounded-md border border-border bg-bg-primary py-1 shadow-float">
          {results.length === 0 ? (
            <p role="status" className="px-3 py-2 text-[12px] text-text-tertiary">
              {t('settings.searchNoResults')}
            </p>
          ) : (
            <ul id={listId} role="listbox" className="m-0 list-none p-0">
              {results.map((result, index) => (
                <li
                  key={`${result.pane}-${result.key}`}
                  id={optionId(index)}
                  role="option"
                  aria-selected={index === active}
                  // Keep focus in the box so the list stays open while choosing.
                  onMouseDown={(event) => event.preventDefault()}
                  onMouseEnter={() => setActive(index)}
                  onClick={() => pick(index)}
                  className={`cursor-pointer px-3 py-1.5 transition-colors duration-150 ${
                    index === active ? 'bg-bg-secondary' : ''
                  }`}
                >
                  <span className="block text-[12px] text-text-primary">{result.label}</span>
                  <span className="block text-[11px] text-text-tertiary">{result.paneLabel}</span>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  )
}
