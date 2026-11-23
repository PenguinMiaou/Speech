import { useState, useEffect, useRef } from 'react'
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
import { useDirtyConfig } from './shared/useDirtyConfig'

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
  const { t } = useTranslation()

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

  return (
    <div className="w-full h-full bg-bg-primary text-text-primary flex flex-col">
      <div className="flex-1 flex min-h-0">
        {/* Sidebar */}
        <SettingsSidebar activePane={activePane} onSelect={setActivePane} />

        {/* Content */}
        <div className="flex-1 flex flex-col min-w-0">
          {/* Title bar */}
          <div className="page-header">
            <h2 className="page-title">{t(paneTitleKeys[activePane])}</h2>
          </div>

          {/* Pane content */}
          <div ref={contentRef} className="flex-1 overflow-y-auto overflow-x-hidden px-8 pb-8">
            <motion.div
              key={activePane}
              className="w-full max-w-[720px]"
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.1, ease: 'easeOut' }}
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
      <AnimatePresence>{isDirty && <DirtyBar />}</AnimatePresence>
    </div>
  )
}
