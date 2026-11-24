import { useEffect, useRef, useState } from 'react'
import { Home, Settings, History, AudioLines, ArrowUp } from 'lucide-react'
import { motion, useAnimate } from 'framer-motion'
import { useTranslation } from 'react-i18next'
import { duration, ease, spring } from '../../lib/animations'
import { useRoute, NAV_AGAIN, type Route } from '../../lib/router'
import { useMeetingStore } from '../../stores/meetingStore'
import { meetingRecordLevel } from '../../lib/tauri'
import { clock } from '../../lib/meeting-text'
import { AccessibilityBanner } from './AccessibilityBanner'
import { UpdateBanner } from './UpdateBanner'

const baseNavItems: { id: Route; labelKey: string; icon: typeof Home }[] = [
  { id: 'home', labelKey: 'nav.home', icon: Home },
  { id: 'meeting', labelKey: 'nav.meeting', icon: AudioLines },
  { id: 'history', labelKey: 'nav.history', icon: History },
  { id: 'settings', labelKey: 'nav.settings', icon: Settings },
]

interface Props {
  children: React.ReactNode
}

/** How long the meeting in hand has been recorded, and whether it is paused, read from
 * the recording itself, so that it shows on every page; null when none is recorded. */
function useRecording(): { seconds: number; paused: boolean } | null {
  const recording = useMeetingStore((state) => state.phase === 'recording')
  const [level, setLevel] = useState<{ seconds: number; paused: boolean } | null>(null)
  useEffect(() => {
    if (!recording) return setLevel(null)
    let live = true
    const read = () =>
      void Promise.resolve(meetingRecordLevel())
        .then((now) => {
          if (live && now) setLevel({ seconds: now.seconds, paused: Boolean(now.paused) })
        })
        .catch(() => {})
    read()
    const timer = window.setInterval(read, 1000)
    return () => {
      live = false
      window.clearInterval(timer)
    }
  }, [recording])
  return recording ? (level ?? { seconds: 0, paused: false }) : null
}

/** Whether the window is narrow enough that the sidebar keeps to its icons. */
function useNarrow(): boolean {
  const query = '(max-width: 899px)'
  const [narrow, setNarrow] = useState(() => window.matchMedia?.(query).matches ?? false)
  useEffect(() => {
    const list = window.matchMedia?.(query)
    if (!list) return
    const change = () => setNarrow(list.matches)
    list.addEventListener('change', change)
    return () => list.removeEventListener('change', change)
  }, [])
  return narrow
}

export function MainLayout({ children }: Props) {
  const { route, navigate } = useRoute()
  const { t } = useTranslation()
  const recording = useRecording()
  const narrow = useNarrow()
  // Ctrl or Cmd with 1 to 4 opens a page, with a comma the settings, and with F the
  // search of the page in view, where the page does not find on its own.
  useEffect(() => {
    const press = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.defaultPrevented) return
      const page = Number(event.key)
      if (page >= 1 && page <= baseNavItems.length) navigate(baseNavItems[page - 1].id)
      else if (event.key === ',') navigate('settings')
      else if (event.key.toLowerCase() === 'f') {
        const search = content.current?.querySelector<HTMLInputElement>(
          '[data-find], input[type="search"]',
        )
        if (!search) return
        search.focus()
        search.select()
      } else return
      event.preventDefault()
    }
    window.addEventListener('keydown', press)
    return () => window.removeEventListener('keydown', press)
  }, [navigate])
  const content = useRef<HTMLDivElement>(null)
  /** Whether the page is scrolled down far enough to offer the way back up. */
  const [down, setDown] = useState(false)
  const toTop = () => content.current?.scrollTo({ top: 0, behavior: 'smooth' })
  // A page comes in the same way whichever it is: a short fade.
  const [page, animatePage] = useAnimate<HTMLDivElement>()
  const shown = useRef(route)
  useEffect(() => {
    if (shown.current === route) return
    shown.current = route
    content.current?.scrollTo({ top: 0 })
    setDown(false)
    if (page.current)
      void animatePage(
        page.current,
        // Opacity alone: a transform left on the page would hold its fixed dialogs.
        { opacity: [0, 1] },
        { duration: duration.base, ease },
      )
  }, [route, animatePage, page])

  return (
    <div className="w-full h-full flex bg-bg-primary text-text-primary">
      {/* Sidebar — jelly surface */}
      <aside
        className={`flex shrink-0 flex-col border-r border-border jelly-surface-flat transition-[width] duration-200 ${
          narrow ? 'w-[60px]' : 'w-[200px]'
        }`}
      >
        {/* Logo */}
        <div
          className={narrow ? 'px-2 pb-4 pt-5 text-center' : 'px-5 pb-4 pt-5'}
          data-tauri-drag-region
        >
          <h1 className={`wordmark leading-none ${narrow ? 'text-[22px]' : 'text-[26px]'}`}>
            {narrow ? t('app.name').slice(0, 1) : t('app.name')}
          </h1>
          {!narrow && <p className="text-[11px] text-text-tertiary mt-1.5">{t('app.tagline')}</p>}
        </div>

        {/* Main Nav */}
        <nav
          className={`relative flex-1 space-y-0.5 ${narrow ? 'px-2' : 'px-3'}`}
          aria-label={t('nav.mainNavigation')}
        >
          {baseNavItems.map(({ id, labelKey, icon: Icon }) => {
            const active = route === id
            const label = t(labelKey)
            return (
              <motion.button
                key={id}
                onClick={() => {
                  if (id !== route) return navigate(id)
                  // The page in view, chosen again: back to where it begins.
                  window.dispatchEvent(new CustomEvent(NAV_AGAIN, { detail: id }))
                  toTop()
                }}
                aria-label={label}
                aria-current={active ? 'page' : undefined}
                title={narrow ? label : undefined}
                className={`flex items-center gap-2.5 w-full py-2 text-[13px] ${narrow ? 'justify-center px-0' : 'px-3'} rounded-md transition-colors bg-transparent border-none cursor-pointer text-left relative ${
                  active
                    ? 'text-text-primary font-medium'
                    : 'text-text-secondary hover:text-text-primary'
                }`}
              >
                {active && (
                  <motion.div
                    layoutId="nav-indicator"
                    className="absolute inset-0 jelly-nav-active"
                    transition={spring.gentle}
                  />
                )}
                <span
                  className={`relative z-10 flex items-center gap-2.5 ${narrow ? 'justify-center' : 'w-full'}`}
                >
                  <Icon size={16} className={active ? 'text-accent' : undefined} />
                  {narrow ? <span className="sr-only">{label}</span> : label}
                  {id === 'meeting' && recording && narrow && (
                    <span
                      aria-hidden
                      className={`absolute -right-1 -top-0.5 h-1.5 w-1.5 rounded-full ${
                        recording.paused ? 'bg-warning' : 'recording-dot bg-error'
                      }`}
                    />
                  )}
                  {id === 'meeting' && recording && !narrow && (
                    <span
                      className="ml-auto flex items-center gap-1.5 text-[11px] tabular-nums text-text-secondary"
                      title={recording.paused ? t('nav.meetingPaused') : t('nav.meetingRecording')}
                    >
                      <span
                        aria-hidden
                        className={`h-1.5 w-1.5 rounded-full ${
                          recording.paused ? 'bg-warning' : 'recording-dot bg-error'
                        }`}
                      />
                      <span className="sr-only">
                        {recording.paused ? t('nav.meetingPaused') : t('nav.meetingRecording')}
                      </span>
                      {clock(recording.seconds)}
                    </span>
                  )}
                </span>
              </motion.button>
            )
          })}
        </nav>
      </aside>

      {/* Content */}
      <main className="flex-1 min-w-0 flex flex-col">
        <UpdateBanner />
        <AccessibilityBanner />
        <div className="relative min-h-0 flex-1">
          <div
            ref={content}
            onScroll={(event) => setDown(event.currentTarget.scrollTop > 600)}
            className="h-full overflow-y-auto"
          >
            <div ref={page} className="h-full">
              {children}
            </div>
          </div>
          {down && (
            <button
              type="button"
              onClick={toTop}
              aria-label={t('nav.toTop')}
              title={t('nav.toTop')}
              className="absolute bottom-5 right-5 z-20 grid h-9 w-9 cursor-pointer place-items-center rounded-full border border-border bg-bg-primary text-text-secondary shadow-float hover:text-text-primary"
            >
              <ArrowUp size={16} />
            </button>
          )}
        </div>
      </main>
    </div>
  )
}
