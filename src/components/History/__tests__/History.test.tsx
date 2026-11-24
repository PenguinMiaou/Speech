import React from 'react'
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useAppStore, type HistoryEntry } from '../../../stores/appStore'
import { addCorrectionRule, clearHistory, getCorrectionRules, getHistory } from '../../../lib/tauri'
import { History } from '../index'

vi.mock('framer-motion', () => ({
  AnimatePresence: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  motion: new Proxy(
    {},
    {
      get:
        (_target, tag: string) =>
        ({ children, ...props }: React.HTMLAttributes<HTMLElement>) =>
          React.createElement(tag, props, children),
    },
  ),
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, unknown>) =>
      values ? `${key} ${Object.values(values).join(' ')}` : key,
    i18n: { language: 'en' },
  }),
}))

vi.mock('../../../lib/tauri', () => ({
  addCorrectionRule: vi.fn().mockResolvedValue(undefined),
  copyText: vi.fn().mockResolvedValue(undefined),
  clearHistory: vi.fn().mockResolvedValue(undefined),
  getCorrectionRules: vi.fn().mockResolvedValue([]),
  getHistory: vi.fn().mockResolvedValue([]),
}))

const entry: HistoryEntry = {
  id: 1,
  created_at: new Date().toISOString(),
  context_profile_id: 'chat.slack',
  context_label: 'Slack',
  context_icon_key: 'slack',
  context_family: 'work_chat',
  browser_access_status: 'not_applicable',
  provider_kind: 'local',
  raw_text: 'open type less',
  polished_text: 'OpenTypeless',
  language: 'en',
  duration_ms: 1000,
  active_scene_id: null,
  active_scene_source: null,
  active_scene_name: null,
  active_scene_prompt_chars: null,
  active_scene_prompt_truncated: false,
  output_status: null,
  output_error: null,
}

describe('History correction creation', () => {
  beforeEach(() => {
    useAppStore.setState({ history: [entry], correctionRules: [] })
    vi.clearAllMocks()
    vi.mocked(addCorrectionRule).mockResolvedValue(undefined)
    vi.mocked(getCorrectionRules).mockResolvedValue([
      { id: 2, pattern: 'open type less', replacement: 'OpenTypeless', enabled: true },
    ])
  })

  afterEach(() => {
    cleanup()
    useAppStore.setState(useAppStore.getInitialState())
  })

  it('does not infer a correction until the user explicitly opens and saves it', async () => {
    render(<History />)
    expect(addCorrectionRule).not.toHaveBeenCalled()

    const menuButton = screen.getByRole('button', { name: 'history.moreActions' })
    fireEvent.click(menuButton)
    fireEvent.click(screen.getByRole('menuitem', { name: 'history.createCorrection' }))

    const dialog = screen.getByRole('dialog', { name: 'history.createCorrection' })
    expect(dialog).toBeInTheDocument()
    expect(screen.getByLabelText('dictionary.wrongPhrase')).toHaveValue('open type less')
    expect(screen.getByLabelText('dictionary.correctPhrase')).toHaveValue('OpenTypeless')
    expect(addCorrectionRule).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: 'history.saveCorrection' }))
    await waitFor(() => {
      expect(addCorrectionRule).toHaveBeenCalledWith('open type less', 'OpenTypeless')
      expect(getCorrectionRules).toHaveBeenCalled()
      expect(useAppStore.getState().correctionRules).toHaveLength(1)
    })
  })

  it('leaves fields empty instead of truncating an overlong history source', () => {
    useAppStore.setState({
      history: [{ ...entry, raw_text: 'x'.repeat(121), polished_text: 'y'.repeat(121) }],
    })
    render(<History />)

    fireEvent.click(screen.getByRole('button', { name: 'history.moreActions' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'history.createCorrection' }))

    expect(screen.getByLabelText('dictionary.wrongPhrase')).toHaveValue('')
    expect(screen.getByLabelText('dictionary.correctPhrase')).toHaveValue('')
    expect(screen.getByLabelText('dictionary.wrongPhrase')).toHaveFocus()
  })

  it('closes the row menu on Escape and restores its trigger focus', async () => {
    render(<History />)
    const menuButton = screen.getByRole('button', { name: 'history.moreActions' })

    fireEvent.click(menuButton)
    expect(screen.getByRole('menu')).toBeInTheDocument()
    fireEvent.keyDown(window, { key: 'Escape' })

    expect(screen.queryByRole('menu')).toBeNull()
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'history.moreActions' })).toHaveFocus(),
    )
  })

  it('clears history through an in-app confirmation instead of window.confirm', async () => {
    const confirmSpy = vi.spyOn(window, 'confirm')
    render(<History />)

    fireEvent.click(screen.getByRole('button', { name: 'history.clearAll' }))
    expect(confirmSpy).not.toHaveBeenCalled()
    expect(screen.getByText('history.clearConfirm')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'history.confirmClear' }))

    await waitFor(() => {
      expect(clearHistory).toHaveBeenCalled()
      expect(useAppStore.getState().history).toEqual([])
    })
    confirmSpy.mockRestore()
  })
})

/** The process's environment, where the zone the clock is read in is set. */
const env = (globalThis as unknown as { process: { env: Record<string, string | undefined> } })
  .process.env

describe('History days', () => {
  afterEach(() => {
    vi.useRealTimers()
    delete env.TZ
    cleanup()
  })

  it('names today and yesterday by the local day the entries were made on', () => {
    env.TZ = 'Asia/Singapore'
    vi.useFakeTimers({ toFake: ['Date'] })
    // 01:30 on the 24th here, still the 23rd in UTC.
    vi.setSystemTime(new Date('2026-11-23T17:30:00Z'))
    useAppStore.setState({
      history: [
        { ...entry, id: 1, created_at: '2026-11-24T01:00:00' },
        { ...entry, id: 2, created_at: '2026-11-23T20:00:00' },
      ],
      correctionRules: [],
    })
    render(<History />)
    expect(screen.getByText('history.today')).toBeTruthy()
    expect(screen.getByText('history.yesterday')).toBeTruthy()
  })
})

describe('History search, filter and paging', () => {
  const notes = { ...entry, id: 10, context_label: 'Notes', context_icon_key: 'notes' }
  const mail = { ...entry, id: 11, context_label: 'Mail', context_icon_key: 'mail' }

  beforeEach(() => {
    vi.clearAllMocks()
    useAppStore.setState({
      history: [
        { ...notes, polished_text: 'Review the Plan on Friday' },
        { ...mail, polished_text: 'Send the weekly summary' },
      ],
      correctionRules: [],
    })
  })

  afterEach(() => {
    cleanup()
    useAppStore.setState(useAppStore.getInitialState())
  })

  it('matches regardless of case and marks the match in the text', () => {
    render(<History />)
    fireEvent.change(screen.getByLabelText('history.searchPlaceholder'), {
      target: { value: 'PLAN' },
    })
    expect(screen.queryByText(/weekly summary/)).toBeNull()
    const mark = document.querySelector('mark')
    expect(mark?.textContent).toBe('Plan')
  })

  it('filters by the app an entry was dictated into', () => {
    render(<History />)
    const select = screen.getByLabelText('history.filterByApp')
    fireEvent.change(select, { target: { value: 'Mail' } })
    expect(screen.getByText('Send the weekly summary')).toBeTruthy()
    expect(screen.queryByText('Review the Plan on Friday')).toBeNull()
  })

  it('shows a no-results state with a way to clear the search', () => {
    render(<History />)
    fireEvent.change(screen.getByLabelText('history.searchPlaceholder'), {
      target: { value: 'nothing like this' },
    })
    expect(screen.getByRole('status').textContent).toContain('history.noResults')
    fireEvent.click(screen.getByRole('button', { name: 'history.clearFilters' }))
    expect(screen.getByText('Send the weekly summary')).toBeTruthy()
  })

  it('labels the copy button through i18n and keeps it reachable by keyboard', () => {
    render(<History />)
    const copy = screen.getAllByRole('button', { name: /^history\.copyEntry/ })[0]
    expect(copy.className).toContain('opacity-40')
    expect(copy.className).toContain('focus-visible:opacity-100')
  })

  it('writes older days out in the interface language', () => {
    useAppStore.setState({ history: [{ ...notes, created_at: '2025-03-07T09:00:00' }] })
    render(<History />)
    const expected = new Intl.DateTimeFormat('en', {
      weekday: 'short',
      month: 'long',
      day: 'numeric',
      year: 'numeric',
    }).format(new Date(2025, 2, 7))
    expect(screen.getByRole('heading', { level: 3 }).textContent).toBe(expected)
  })

  it('loads the next page after a full first page and stops at a short one', async () => {
    const first = Array.from({ length: 200 }, (_, i) => ({ ...notes, id: 1000 - i }))
    useAppStore.setState({ history: first })
    vi.mocked(getHistory).mockResolvedValue([
      { ...notes, id: 801 }, // already shown
      { ...mail, id: 700, polished_text: 'An older note' },
    ])
    render(<History />)
    // Found by text: looking up roles across 200 rows is slow.
    fireEvent.click(screen.getByText('history.loadMore'))
    await waitFor(() => expect(screen.getByText('An older note')).toBeTruthy())
    expect(getHistory).toHaveBeenCalledWith(200, 200)
    expect(useAppStore.getState().history).toHaveLength(201)
    expect(screen.queryByText('history.loadMore')).toBeNull()

    // The refresh after a new dictation puts back the first page; older entries can load again.
    act(() => useAppStore.setState({ history: first }))
    expect(screen.getByText('history.loadMore')).toBeTruthy()
  }, 20_000) // Rendering a full page of 200 rows is slow under jsdom.

  it('offers no load more when the first page is not full', () => {
    render(<History />)
    expect(screen.queryByRole('button', { name: 'history.loadMore' })).toBeNull()
  })
})
