import React from 'react'
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { MainLayout } from '../index'
import { useMeetingStore, initialMeeting } from '../../../stores/meetingStore'
import { meetingRecordLevel } from '../../../lib/tauri'

vi.mock('../../../lib/tauri', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../lib/tauri')>()),
  meetingRecordLevel: vi.fn(),
}))

const MOTION_PROPS = new Set([
  'initial',
  'animate',
  'exit',
  'transition',
  'variants',
  'whileHover',
  'whileTap',
  'layoutId',
  'layout',
])

vi.mock('framer-motion', () => ({
  useAnimate: () => [{ current: null }, () => Promise.resolve()],
  AnimatePresence: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  motion: new Proxy(
    {},
    {
      get:
        (_target, tag: string) =>
        ({ children, ...props }: React.HTMLAttributes<HTMLElement>) => {
          const domProps: Record<string, unknown> = {}
          for (const [key, value] of Object.entries(props)) {
            if (!MOTION_PROPS.has(key)) domProps[key] = value
          }
          return React.createElement(tag, domProps, children)
        },
    },
  ),
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) =>
      ({
        'app.name': 'Speech',
        'app.tagline': 'AI Voice Input',
        'nav.home': 'Home',
        'nav.meeting': 'Meetings',
        'nav.ask': 'Ask',
        'nav.settings': 'Settings',
        'nav.history': 'History',
        'nav.upgrade': 'Upgrade',
        'nav.account': 'Account',
        'nav.mainNavigation': 'Main navigation',
      })[key] ?? key,
  }),
}))

afterEach(() => {
  cleanup()
  window.location.hash = ''
})

describe('MainLayout', () => {
  it('does not show Ask as a first-class navigation item', () => {
    render(
      <MainLayout>
        <div>content</div>
      </MainLayout>,
    )

    expect(screen.queryByRole('button', { name: 'Ask' })).not.toBeInTheDocument()
  })

  it('has no upgrade or account entry', () => {
    render(
      <MainLayout>
        <div />
      </MainLayout>,
    )
    expect(screen.queryByText('Upgrade')).not.toBeInTheDocument()
    expect(screen.queryByText('Account')).not.toBeInTheDocument()
  })

  it('lists meetings between home and history', () => {
    render(<MainLayout>content</MainLayout>)
    const names = screen.getAllByRole('button').map((button) => button.getAttribute('aria-label'))
    expect(names).toEqual(['Home', 'Meetings', 'History', 'Settings'])
  })

  it('sets the app name in the wordmark face', () => {
    render(<MainLayout>content</MainLayout>)
    expect(screen.getByRole('heading', { name: 'Speech' }).className).toContain('wordmark')
  })

  it('takes the page in view back to its beginning when it is chosen again', () => {
    window.location.hash = '#/meeting'
    const again = vi.fn()
    window.addEventListener('nav:again', again)
    const view = render(
      <MainLayout>
        <div style={{ height: 5000 }}>long page</div>
      </MainLayout>,
    )
    const scroller = view.getByText('long page').parentElement?.parentElement as HTMLDivElement
    scroller.scrollTo = vi.fn()
    // Far down the page, the way back up is offered.
    expect(screen.queryByRole('button', { name: 'nav.toTop' })).toBeNull()
    scroller.scrollTop = 1200
    fireEvent.scroll(scroller)
    fireEvent.click(screen.getByRole('button', { name: 'nav.toTop' }))
    expect(scroller.scrollTo).toHaveBeenCalledWith({ top: 0, behavior: 'smooth' })

    fireEvent.click(screen.getByRole('button', { name: 'Meetings' }))
    expect(again).toHaveBeenCalledTimes(1)
    expect((again.mock.calls[0][0] as CustomEvent).detail).toBe('meeting')
    // Another page is gone to, not sent back.
    fireEvent.click(screen.getByRole('button', { name: 'History' }))
    expect(again).toHaveBeenCalledTimes(1)
    expect(window.location.hash).toBe('#/history')
    window.removeEventListener('nav:again', again)
  })

  it('shows on every page that a meeting is being recorded, and for how long', async () => {
    vi.mocked(meetingRecordLevel).mockResolvedValue({
      seconds: 125,
      volume: 0,
      waiting: 0,
      paused: false,
    })
    window.location.hash = '#/settings'
    useMeetingStore.setState({ ...initialMeeting, phase: 'recording' })
    render(
      <MainLayout>
        <div />
      </MainLayout>,
    )
    expect(await screen.findByText('02:05')).toBeTruthy()
    expect(screen.getByText('nav.meetingRecording')).toBeTruthy()
    act(() => useMeetingStore.setState({ phase: 'idle' }))
    expect(screen.queryByText('02:05')).toBeNull()
  })

  it('opens pages and the page search from the keyboard', () => {
    window.location.hash = '#/'
    render(
      <MainLayout>
        <input type="search" aria-label="page search" />
      </MainLayout>,
    )
    fireEvent.keyDown(window, { key: '2', ctrlKey: true })
    expect(window.location.hash).toBe('#/meeting')
    fireEvent.keyDown(window, { key: ',', metaKey: true })
    expect(window.location.hash).toBe('#/settings')
    fireEvent.keyDown(window, { key: 'f', ctrlKey: true })
    expect(document.activeElement).toBe(screen.getByLabelText('page search'))
    // Without Ctrl or Cmd a number is typed, not a page opened.
    fireEvent.keyDown(window, { key: '1' })
    expect(window.location.hash).toBe('#/settings')
  })

  it('keeps the sidebar to its icons in a narrow window, the names still read out', () => {
    const matchMedia = window.matchMedia
    window.matchMedia = ((query: string) => ({
      matches: query.includes('max-width'),
      addEventListener: () => {},
      removeEventListener: () => {},
    })) as unknown as typeof window.matchMedia
    render(
      <MainLayout>
        <div />
      </MainLayout>,
    )
    const meetings = screen.getByRole('button', { name: 'Meetings' })
    expect(meetings.getAttribute('title')).toBe('Meetings')
    expect(screen.queryByText('AI Voice Input')).toBeNull()
    window.matchMedia = matchMedia
  })
})
