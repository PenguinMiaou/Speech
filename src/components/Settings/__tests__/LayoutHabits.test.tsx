import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { LayoutHabits } from '../LayoutHabits'
import { forgetLayoutHabit, listLayoutHabits } from '../../../lib/tauri'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

vi.mock('../../../lib/tauri', () => ({
  listLayoutHabits: vi.fn(),
  forgetLayoutHabit: vi.fn().mockResolvedValue(undefined),
}))

const habit = (app: string, name: string, active: boolean) => ({
  app,
  habit: name,
  changed: active ? 5 : 2,
  kept: 0,
  active,
})

describe('LayoutHabits', () => {
  beforeEach(() => vi.clearAllMocks())
  afterEach(cleanup)

  it('lists only the habits that took effect, by application', async () => {
    vi.mocked(listLayoutHabits).mockResolvedValue([
      habit('com.example.chat', 'no_final_period', true),
      habit('com.example.mail', 'one_paragraph', false),
    ])
    render(<LayoutHabits />)
    await screen.findByText('com.example.chat')
    expect(screen.getByText('dictionary.habit.no_final_period')).toBeTruthy()
    expect(screen.queryByText('com.example.mail')).toBeNull()
  })

  it('shows nothing when no habit took effect', async () => {
    vi.mocked(listLayoutHabits).mockResolvedValue([
      habit('com.example.mail', 'one_paragraph', false),
    ])
    const { container } = render(<LayoutHabits />)
    await waitFor(() => expect(listLayoutHabits).toHaveBeenCalled())
    expect(container.textContent).toBe('')
  })

  it('forgets a habit and takes it off the list', async () => {
    vi.mocked(listLayoutHabits).mockResolvedValue([
      habit('com.example.chat', 'no_final_period', true),
    ])
    render(<LayoutHabits />)
    fireEvent.click(await screen.findByRole('button', { name: 'dictionary.forgetHabit' }))
    await waitFor(() =>
      expect(forgetLayoutHabit).toHaveBeenCalledWith('com.example.chat', 'no_final_period'),
    )
    await waitFor(() => expect(screen.queryByText('com.example.chat')).toBeNull())
  })
})
