import { WhatsNew, markWhatsNewSeen } from './components/WhatsNew'
import { lazy, Suspense, useEffect, useState } from 'react'
import i18n from './i18n'
import { useTauriEvents } from './hooks/useTauriEvents'
import { useTheme } from './hooks/useTheme'
import { useLocalModelActivation } from './hooks/useLocalModelActivation'
import { useAppStore } from './stores/appStore'
import { useRoute } from './lib/router'
import { useMeetingStore } from './stores/meetingStore'
import {
  loadOnboardingCompleted,
  getConfig,
  getHistory,
  getDictionary,
  getCorrectionRules,
  checkAccessibilityPermission,
  getPlatformCapabilities,
  getHotkeyRegistrationError,
  updateCheck,
  onMeetingRecordAsked,
} from './lib/tauri'
import { autoCheckEnabled } from './lib/update'
import { MainLayout } from './components/MainLayout'
import { ContextMenuHost } from './components/ContextMenu'
import { MotionConfig } from 'framer-motion'
import { timed } from './lib/animations'
import { ToastContainer } from './components/Toast'

// Each window reads only what it shows: the capsule, Ask and the offer have their own parts,
// and the main window's pages are read as they are first opened.
const pages = {
  home: () => import('./components/HomePage').then((m) => ({ default: m.HomePage })),
  meeting: () =>
    import('./components/HomePage/MeetingPage').then((m) => ({ default: m.MeetingPage })),
  settings: () => import('./components/Settings').then((m) => ({ default: m.Settings })),
  history: () => import('./components/History').then((m) => ({ default: m.History })),
}
const HomePage = lazy(pages.home)
const MeetingPage = lazy(pages.meeting)
const Settings = lazy(pages.settings)
const History = lazy(pages.history)
const Onboarding = lazy(() =>
  import('./components/Onboarding').then((m) => ({ default: m.Onboarding })),
)
const Capsule = lazy(() => import('./components/Capsule').then((m) => ({ default: m.Capsule })))
const AskPanel = lazy(() => import('./components/AskPanel').then((m) => ({ default: m.AskPanel })))
const MeetingOffer = lazy(() =>
  import('./components/MeetingOffer').then((m) => ({ default: m.MeetingOffer })),
)

function CapsuleApp() {
  useTauriEvents()
  useTheme()

  const setConfig = useAppStore((s) => s.setConfig)

  useEffect(() => {
    // Load config so DurationTimer gets the correct max_recording_seconds
    getConfig()
      .then((config) => {
        setConfig(config)
        // Restore UI language from config
        if (config.ui_language && config.ui_language !== i18n.language) {
          i18n.changeLanguage(config.ui_language)
          localStorage.setItem('ui_language', config.ui_language)
        }
      })
      .catch((e) => {
        console.error('Failed to load config in capsule:', e)
      })
  }, [setConfig])

  // Window show is handled by useCapsuleResize (setSize → setPosition → show),
  // which works on both Windows and macOS. The previous rAF-based show approach
  // failed on macOS because WKWebView pauses requestAnimationFrame in hidden windows.
  return (
    <Suspense fallback={null}>
      <Capsule />
    </Suspense>
  )
}

/** The window that asks whether to record a meeting found going on. */
function OfferApp() {
  useTheme()
  const setConfig = useAppStore((s) => s.setConfig)
  useEffect(() => {
    getConfig()
      .then((config) => {
        setConfig(config)
        if (config.ui_language && config.ui_language !== i18n.language) {
          void i18n.changeLanguage(config.ui_language)
        }
      })
      .catch(() => {})
  }, [setConfig])
  return (
    <Suspense fallback={null}>
      <MeetingOffer />
    </Suspense>
  )
}

function AskApp() {
  useTheme()
  const setConfig = useAppStore((s) => s.setConfig)

  useEffect(() => {
    getConfig()
      .then((config) => {
        setConfig(config)
        if (config.ui_language && config.ui_language !== i18n.language) {
          i18n.changeLanguage(config.ui_language)
          localStorage.setItem('ui_language', config.ui_language)
        }
      })
      .catch((e) => {
        console.error('Failed to load config in Ask app:', e)
      })
  }, [setConfig])

  return (
    <>
      <Suspense fallback={null}>
        <AskPanel />
      </Suspense>
      <ToastContainer />
    </>
  )
}

function MainApp() {
  useTauriEvents()
  useTheme()
  useLocalModelActivation()

  const onboardingCompleted = useAppStore((s) => s.onboardingCompleted)
  const setOnboardingCompleted = useAppStore((s) => s.setOnboardingCompleted)
  const setConfig = useAppStore((s) => s.setConfig)
  const setSavedConfig = useAppStore((s) => s.setSavedConfig)
  const setHistory = useAppStore((s) => s.setHistory)
  const setDictionary = useAppStore((s) => s.setDictionary)
  const setCorrectionRules = useAppStore((s) => s.setCorrectionRules)
  const setAccessibilityTrusted = useAppStore((s) => s.setAccessibilityTrusted)
  const setPlatformCapabilities = useAppStore((s) => s.setPlatformCapabilities)
  const setHotkeyRegistrationError = useAppStore((s) => s.setHotkeyRegistrationError)
  const [loaded, setLoaded] = useState(false)
  const [loadError, setLoadError] = useState(false)
  const { route, navigate } = useRoute()

  // Asked from the offer window to record a meeting going on: the meeting page begins it.
  useEffect(() => {
    const stop = Promise.resolve(
      onMeetingRecordAsked(() => {
        useMeetingStore.setState({ recordAsked: true })
        navigate('meeting')
      }),
    )
    return () => void stop.then((unlisten) => unlisten?.())
  }, [navigate])

  useEffect(() => {
    loadOnboardingCompleted().then(async (done) => {
      setOnboardingCompleted(done)
      // A first run has nothing new to tell of: only updates show what they changed.
      if (!done) markWhatsNewSeen()
      try {
        const [
          config,
          history,
          dictionary,
          correctionRules,
          platformCapabilities,
          hotkeyRegistrationError,
        ] = await Promise.all([
          getConfig(),
          getHistory(200, 0),
          getDictionary(),
          getCorrectionRules(),
          getPlatformCapabilities(),
          getHotkeyRegistrationError(),
        ])
        setConfig(config)
        setSavedConfig(config)
        setHistory(history)
        setDictionary(dictionary)
        setCorrectionRules(correctionRules)
        setPlatformCapabilities(platformCapabilities)
        setHotkeyRegistrationError(hotkeyRegistrationError)
        if (autoCheckEnabled()) {
          updateCheck()
            .then((status) => useAppStore.getState().setUpdateStatus(status))
            .catch(() => {})
        }
        // Check macOS Accessibility permission
        if (navigator.platform.toUpperCase().indexOf('MAC') >= 0) {
          checkAccessibilityPermission().then((trusted) => {
            setAccessibilityTrusted(trusted)
          })
        }
        // Restore UI language from config
        if (config.ui_language && config.ui_language !== i18n.language) {
          i18n.changeLanguage(config.ui_language)
          localStorage.setItem('ui_language', config.ui_language)
        }
      } catch (e) {
        console.error('Failed to load initial data:', e)
        setLoadError(true)
      }
      setLoaded(true)
      // The other pages are read while nothing else is going on, so none waits when opened.
      const later =
        window.requestIdleCallback ?? ((run: () => void) => window.setTimeout(run, 1500))
      later(() => Object.values(pages).forEach((page) => void page().catch(() => {})))
    })
  }, [
    setOnboardingCompleted,
    setConfig,
    setSavedConfig,
    setHistory,
    setDictionary,
    setCorrectionRules,
    setAccessibilityTrusted,
    setPlatformCapabilities,
    setHotkeyRegistrationError,
  ])

  if (!loaded)
    return (
      <div className="flex items-center justify-center h-screen">
        <span className="text-text-tertiary text-[13px]">Loading...</span>
      </div>
    )
  if (loadError)
    return (
      <div className="flex flex-col items-center justify-center h-screen gap-3">
        <span className="text-error text-[13px]">Failed to load application data.</span>
        <button
          onClick={() => window.location.reload()}
          className="px-4 py-2 bg-accent text-white rounded-lg text-[13px] border-none cursor-pointer hover:bg-accent-hover transition-colors"
        >
          Retry
        </button>
      </div>
    )
  if (!onboardingCompleted)
    return (
      <Suspense fallback={null}>
        <Onboarding />
      </Suspense>
    )

  return (
    <MainLayout>
      <Suspense fallback={null}>
        {route === 'home' && <HomePage />}
        {route === 'meeting' && <MeetingPage />}
        {route === 'settings' && <Settings />}
        {route === 'history' && <History />}
      </Suspense>
      <WhatsNew />
      <ToastContainer />
    </MainLayout>
  )
}

function App() {
  // Capsule window loads with #capsule hash — detect synchronously, no race condition
  if (window.location.hash === '#capsule')
    return (
      <MotionConfig reducedMotion="user" transition={timed()}>
        <CapsuleApp />
      </MotionConfig>
    )
  // Every other window shows the app's own right-click menu, in place of the web view's.
  return (
    <MotionConfig reducedMotion="user" transition={timed()}>
      {window.location.hash === '#ask' ? (
        <AskApp />
      ) : window.location.hash === '#offer' ? (
        <OfferApp />
      ) : (
        <MainApp />
      )}
      <ContextMenuHost />
    </MotionConfig>
  )
}

export default App
