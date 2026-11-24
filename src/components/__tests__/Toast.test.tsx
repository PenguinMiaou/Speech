import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ToastContainer } from '../Toast'
import { toast } from '../toast-service'

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }))

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

describe('toast', () => {
  it('makes an undoable change for good only once its time is up', () => {
    vi.useFakeTimers()
    render(<ToastContainer />)
    const commit = vi.fn()
    const undo = vi.fn()
    act(() => {
      toast.undoable('Meeting deleted', { label: 'Undo', undo, commit, ms: 5000 })
    })
    expect(screen.getByRole('status').textContent).toContain('Meeting deleted')
    act(() => vi.advanceTimersByTime(4999))
    expect(commit).not.toHaveBeenCalled()
    act(() => vi.advanceTimersByTime(1))
    expect(commit).toHaveBeenCalledTimes(1)
    expect(undo).not.toHaveBeenCalled()
  })

  it('undoes instead, and then never makes the change', () => {
    vi.useFakeTimers()
    render(<ToastContainer />)
    const commit = vi.fn()
    const undo = vi.fn()
    act(() => {
      toast.undoable('Cut', { label: 'Undo', undo, commit, ms: 5000 })
    })
    fireEvent.click(screen.getByRole('button', { name: 'Undo' }))
    act(() => vi.advanceTimersByTime(10_000))
    expect(undo).toHaveBeenCalledTimes(1)
    expect(commit).not.toHaveBeenCalled()
  })

  it('makes the change even where no toast can be shown, and can be made at once', () => {
    vi.useFakeTimers()
    const commit = vi.fn()
    const now = toast.undoable('Deleted', { label: 'Undo', undo: vi.fn(), commit, ms: 5000 })
    now()
    now()
    act(() => vi.advanceTimersByTime(10_000))
    expect(commit).toHaveBeenCalledTimes(1)
  })

  it('announces an error at once and other news politely', () => {
    render(<ToastContainer />)
    act(() => {
      toast.error('Could not delete')
      toast.success('Copied')
    })
    expect(screen.getByRole('alert').textContent).toContain('Could not delete')
    expect(screen.getByRole('status').textContent).toContain('Copied')
  })
})
