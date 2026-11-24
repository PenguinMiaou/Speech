import { useRef, useState } from 'react'
import {
  Settings,
  Keyboard,
  Mic,
  PenLine,
  Languages,
  AudioLines,
  BookOpen,
  LayoutGrid,
  Info,
  Search,
} from 'lucide-react'
import { motion } from 'framer-motion'
import { useTranslation } from 'react-i18next'
import { spring } from '../../lib/animations'
import { useAppStore } from '../../stores/appStore'
import { SettingsSearch } from './SettingsSearch'

export const PANES = [
  { id: 'general', labelKey: 'settings.general', icon: Settings },
  { id: 'dictation', labelKey: 'settings.dictationPane', icon: Keyboard },
  { id: 'stt', labelKey: 'settings.speechRecognition', icon: Mic },
  { id: 'llm', labelKey: 'settings.aiPolish', icon: PenLine },
  { id: 'translation', labelKey: 'settings.translationPane', icon: Languages },
  { id: 'meetings', labelKey: 'nav.meeting', icon: AudioLines },
  { id: 'dictionary', labelKey: 'settings.dictionary', icon: BookOpen },
  { id: 'scenes', labelKey: 'settings.scenes', icon: LayoutGrid },
  { id: 'about', labelKey: 'settings.about', icon: Info },
] as const

export type PaneId = (typeof PANES)[number]['id']

interface Props {
  activePane: PaneId
  onSelect: (id: PaneId) => void
  /** Opens a pane from a search result and points at the setting found. */
  onSearchPick: (id: PaneId, label: string) => void
}

/** Below 900px window width the sidebar folds to a rail of icons, and the search to a button
 * that opens it in a small panel. */
export function SettingsSidebar({ activePane, onSelect, onSearchPick }: Props) {
  const { t } = useTranslation()
  const updateAvailable = useAppStore((s) => s.updateStatus?.available ?? false)
  const [searchOpen, setSearchOpen] = useState(false)
  const searchButton = useRef<HTMLButtonElement>(null)

  const closeSearch = () => {
    setSearchOpen(false)
    searchButton.current?.focus()
  }

  return (
    <div className="relative w-[200px] max-[899px]:w-[56px] shrink-0 h-full jelly-surface-flat border-r border-border flex flex-col py-4 px-2 gap-0.5">
      <h2 className="text-[13px] font-semibold text-text-primary px-3 pb-3 max-[899px]:sr-only">
        {t('settings.title')}
      </h2>
      <div className="px-1 pb-2 max-[899px]:hidden">
        <SettingsSearch onPick={onSearchPick} />
      </div>
      <div className="hidden max-[899px]:block pb-2">
        <button
          ref={searchButton}
          type="button"
          onClick={() => setSearchOpen((open) => !open)}
          aria-label={t('settings.searchPlaceholder')}
          title={t('settings.searchPlaceholder')}
          aria-expanded={searchOpen}
          className="flex w-full items-center justify-center rounded-md border-none bg-transparent py-2 text-text-secondary cursor-pointer hover:text-text-primary hover:bg-bg-secondary transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
        >
          <Search size={16} />
        </button>
        {searchOpen && (
          <>
            <div className="fixed inset-0 z-30" onClick={() => setSearchOpen(false)} />
            <div className="absolute left-[60px] top-12 z-40 w-[240px] rounded-md border border-border bg-bg-primary p-2 shadow-float">
              <SettingsSearch onPick={onSearchPick} onDone={closeSearch} autoFocus />
            </div>
          </>
        )}
      </div>
      {PANES.map((pane) => {
        const Icon = pane.icon
        const isActive = activePane === pane.id
        const label = t(pane.labelKey)
        return (
          <motion.button
            key={pane.id}
            onClick={() => onSelect(pane.id)}
            transition={spring.gentle}
            aria-current={isActive ? 'page' : undefined}
            title={label}
            className={`flex items-center gap-2.5 w-full px-3 py-2 max-[899px]:justify-center max-[899px]:px-0 rounded-md text-[13px] border-none cursor-pointer transition-colors text-left relative focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${
              isActive
                ? 'text-text-primary font-medium'
                : 'bg-transparent text-text-secondary hover:text-text-primary'
            }`}
          >
            {isActive && (
              <motion.div
                layoutId="settings-nav-indicator"
                className="absolute inset-0 jelly-nav-active"
                transition={spring.gentle}
              />
            )}
            <span className="relative z-10 flex items-center gap-2.5">
              <Icon size={16} />
              <span className="max-[899px]:sr-only">{label}</span>
              {pane.id === 'about' && updateAvailable && (
                <span
                  data-testid="update-dot"
                  className="h-1.5 w-1.5 rounded-full bg-accent max-[899px]:absolute max-[899px]:-right-1.5 max-[899px]:-top-0.5"
                />
              )}
            </span>
          </motion.button>
        )
      })}
    </div>
  )
}
