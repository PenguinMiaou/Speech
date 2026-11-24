import { useState, useEffect, useRef, useCallback } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { useTranslation } from 'react-i18next'
import { useAppStore } from '../../stores/appStore'
import { SettingsSidebar, type PaneId } from './SettingsSidebar'
import { GeneralPane } from './GeneralPane'
import { SttPane } from './SttPane'
import { LlmPane } from './LlmPane'
import { DictionaryPane } from './DictionaryPane'
import { ScenesPane } from './ScenesPane'
import { AboutPane } from './AboutPane'
import { DictationPane } from './DictationPane'
import { TranslationPane } from './TranslationPane'
import { MeetingsPane } from './MeetingsPane'
import { DirtyBar } from './shared/DirtyBar'
import { isConfigDirty, useDirtyConfig } from './shared/useDirtyConfig'
import { useSaveConfig } from './shared/useSaveConfig'
import { UnsavedLeaveDialog } from './UnsavedLeaveDialog'
import { parseHash } from '../../lib/router'
import { timed } from '../../lib/animations'
import { findLabel } from './findLabel'

/** Classes that mark a setting found by search for a moment. */
const FOUND_CLASSES = ['bg-accent/10', 'ring-2', 'ring-accent/30', 'rounded-sm']
const FOUND_FOR_MS = 1600

const paneTitleKeys: Record<PaneId, string> = {
  general: 'settings.general',
  dictation: 'settings.dictationPane',
  stt: 'settings.speechRecognition',
  llm: 'settings.aiPolish',
  translation: 'settings.translationPane',
  meetings: 'nav.meeting',
  dictionary: 'settings.dictionary',
  scenes: 'settings.scenes',
  about: 'settings.about',
}

/** Panes that were folded into another, by the one that holds them now. */
const movedPanes: Record<string, PaneId> = { language: 'general' }

function paneFromHash(): PaneId | null {
  const query = window.location.hash.split('?')[1]
  if (!query) return null
  const asked = new URLSearchParams(query).get('pane')
  const requested = asked ? (movedPanes[asked] ?? asked) : null
  return requested && requested in paneTitleKeys ? (requested as PaneId) : null
}

export function Settings() {
  const [activePane, setActivePane] = useState<PaneId>(() => paneFromHash() ?? 'general')
  const contentRef = useRef<HTMLDivElement | null>(null)
  const config = useAppStore((s) => s.config)
  const setSavedConfig = useAppStore((s) => s.setSavedConfig)
  const isDirty = useDirtyConfig()
  const saver = useSaveConfig()
  const { t } = useTranslation()
  /** Where the user was going when Settings held them back to ask about unsaved edits. */
  const [leavingTo, setLeavingTo] = useState<string | null>(null)
  const [found, setFound] = useState<{ label: string; at: number } | null>(null)

  // First-run onboarding may enter Settings before MainApp has established backend truth.
  useEffect(() => {
    if (useAppStore.getState().savedConfig === null) setSavedConfig(config)
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const syncRequestedPane = () => {
      const requested = paneFromHash()
      if (requested) setActivePane(requested)
    }
    window.addEventListener('hashchange', syncRequestedPane)
    return () => window.removeEventListener('hashchange', syncRequestedPane)
  }, [])

  useEffect(() => {
    contentRef.current?.scrollTo?.({ top: 0 })
  }, [activePane])

  // Leaving Settings with unsaved edits: this listener runs in the capture phase, ahead of the
  // router's, puts the address back and asks first.
  useEffect(() => {
    const holdBack = (event: HashChangeEvent) => {
      if (parseHash() === 'settings') return
      const { config, savedConfig } = useAppStore.getState()
      if (!isConfigDirty(config, savedConfig)) return
      event.stopImmediatePropagation()
      window.history.replaceState(window.history.state, '', event.oldURL)
      setLeavingTo(new URL(event.newURL).hash || '#/')
    }
    window.addEventListener('hashchange', holdBack, true)
    return () => window.removeEventListener('hashchange', holdBack, true)
  }, [])

  // Ctrl+S / Cmd+S saves while in Settings.
  const { save } = saver
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // A shortcut being recorded takes the keys for itself.
      if (event.defaultPrevented) return
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.key.toLowerCase() !== 's') {
        return
      }
      event.preventDefault()
      const { config, savedConfig } = useAppStore.getState()
      if (isConfigDirty(config, savedConfig)) void save()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [save])

  // After a search jumps to a pane, point at the setting that was found.
  useEffect(() => {
    if (!found) return
    let element: HTMLElement | null = null
    const show = window.setTimeout(() => {
      const root = contentRef.current
      element = root ? findLabel(root, found.label) : null
      if (!element) return
      element.scrollIntoView?.({ block: 'center', behavior: 'smooth' })
      element.classList.add(...FOUND_CLASSES)
      element.dataset.searchFound = 'true'
    }, 60)
    const hide = window.setTimeout(() => {
      element?.classList.remove(...FOUND_CLASSES)
      delete element?.dataset.searchFound
    }, 60 + FOUND_FOR_MS)
    return () => {
      window.clearTimeout(show)
      window.clearTimeout(hide)
      element?.classList.remove(...FOUND_CLASSES)
      delete element?.dataset.searchFound
    }
  }, [found])

  const onSearchPick = useCallback((pane: PaneId, label: string) => {
    setActivePane(pane)
    setFound({ label, at: Date.now() })
  }, [])

  const leaveNow = (target: string) => {
    setLeavingTo(null)
    window.location.hash = target
  }

  const saveAndLeave = async () => {
    if (!leavingTo) return
    const target = leavingTo
    if (await save()) leaveNow(target)
    else setLeavingTo(null)
  }

  const discardAndLeave = () => {
    if (!leavingTo) return
    saver.reset()
    leaveNow(leavingTo)
  }

  const stay = useCallback(() => setLeavingTo(null), [])

  return (
    <div className="w-full h-full bg-bg-primary text-text-primary flex flex-col">
      <div className="flex-1 flex min-h-0">
        {/* Sidebar */}
        <SettingsSidebar
          activePane={activePane}
          onSelect={setActivePane}
          onSearchPick={onSearchPick}
        />

        {/* Content */}
        <div className="flex-1 flex flex-col min-w-0">
          {/* Title bar */}
          <div className="page-header">
            <h2 className="page-title mx-auto w-full max-w-[720px]">
              {t(paneTitleKeys[activePane])}
            </h2>
          </div>

          {/* Pane content */}
          <div ref={contentRef} className="flex-1 overflow-y-auto overflow-x-hidden px-8 pb-8">
            <motion.div
              key={activePane}
              className="mx-auto w-full max-w-[720px]"
              initial={{ opacity: 0, y: 4 }}
              animate={{ opacity: 1, y: 0 }}
              transition={timed('fast')}
            >
              {activePane === 'general' && <GeneralPane />}
              {activePane === 'dictation' && <DictationPane />}
              {activePane === 'stt' && <SttPane />}
              {activePane === 'llm' && <LlmPane />}
              {activePane === 'translation' && <TranslationPane />}
              {activePane === 'meetings' && <MeetingsPane />}
              {activePane === 'dictionary' && <DictionaryPane />}
              {activePane === 'scenes' && <ScenesPane />}
              {activePane === 'about' && <AboutPane />}
            </motion.div>
          </div>
        </div>
      </div>

      {/* Dirty bar */}
      <AnimatePresence>{isDirty && <DirtyBar saver={saver} />}</AnimatePresence>

      {leavingTo && (
        <UnsavedLeaveDialog
          saving={saver.saving}
          onSave={() => void saveAndLeave()}
          onDiscard={discardAndLeave}
          onStay={stay}
        />
      )}
    </div>
  )
}
