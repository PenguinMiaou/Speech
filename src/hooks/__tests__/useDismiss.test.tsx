import { useRef } from 'react'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useDismiss } from '../useDismiss'

function Popover({ close, outside }: { close: () => void; outside?: boolean }) {
  const inside = useRef<HTMLDivElement>(null)
  useDismiss(true, close, inside, outside)
  return (
    <>
      <div ref={inside}>inside</div>
      <div>elsewhere</div>
      <div data-context-menu>
        <button type="button">Paste</button>
      </div>
    </>
  )
}

afterEach(cleanup)

describe('useDismiss', () => {
  it('closes on Escape and on a press elsewhere, not on one in the right-click menu', () => {
    const close = vi.fn()
    render(<Popover close={close} />)
    fireEvent.mouseDown(screen.getByText('inside'))
    fireEvent.mouseDown(screen.getByText('Paste'))
    expect(close).not.toHaveBeenCalled()
    fireEvent.mouseDown(screen.getByText('elsewhere'))
    expect(close).toHaveBeenCalledTimes(1)
    fireEvent.keyDown(window, { key: 'Escape' })
    expect(close).toHaveBeenCalledTimes(2)
  })

  it('leaves a press elsewhere alone when told to', () => {
    const close = vi.fn()
    render(<Popover close={close} outside={false} />)
    fireEvent.mouseDown(screen.getByText('elsewhere'))
    expect(close).not.toHaveBeenCalled()
  })
})
