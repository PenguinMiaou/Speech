import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ContextMenuHost, openContextMenu } from '../index'

const clipboard = vi.hoisted(() => ({ writeText: vi.fn(), readText: vi.fn() }))
vi.mock('@tauri-apps/plugin-clipboard-manager', () => clipboard)
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }))

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

describe('ContextMenuHost', () => {
  it('shows no menu where nothing can be done, in place of the web view’s', () => {
    render(
      <>
        <p>plain words</p>
        <ContextMenuHost />
      </>,
    )
    const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })
    act(() => {
      screen.getByText('plain words').dispatchEvent(event)
    })
    expect(event.defaultPrevented).toBe(true)
    expect(screen.queryByRole('menu')).toBeNull()
  })

  it('offers editing in a field, and copies what is chosen in it', async () => {
    render(
      <>
        <input aria-label="field" defaultValue="hello world" />
        <ContextMenuHost />
      </>,
    )
    const field = screen.getByLabelText('field') as HTMLInputElement
    field.setSelectionRange(0, 5)
    fireEvent.contextMenu(field)
    expect(screen.getAllByRole('menuitem').map((item) => item.textContent)).toEqual([
      'contextMenu.cut',
      'contextMenu.copy',
      'contextMenu.paste',
      'contextMenu.selectAll',
    ])
    fireEvent.click(screen.getByRole('menuitem', { name: 'contextMenu.copy' }))
    await waitFor(() => expect(clipboard.writeText).toHaveBeenCalledWith('hello'))
    expect(screen.queryByRole('menu')).toBeNull()
  })

  it('shows a page’s own menu, and asks once more before deleting', () => {
    const remove = vi.fn()
    const open = vi.fn()
    render(
      <>
        <div
          onContextMenu={(event) =>
            openContextMenu(event, [
              { label: 'open', onSelect: open },
              { label: 'delete', danger: true, confirm: 'sure?', onSelect: remove },
            ])
          }
        >
          a meeting
        </div>
        <ContextMenuHost />
      </>,
    )
    fireEvent.contextMenu(screen.getByText('a meeting'))
    fireEvent.click(screen.getByRole('menuitem', { name: 'delete' }))
    expect(remove).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('menuitem', { name: 'sure?' }))
    expect(remove).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('menu')).toBeNull()

    // Escape and a click elsewhere close it unused.
    fireEvent.contextMenu(screen.getByText('a meeting'))
    fireEvent.keyDown(window, { key: 'Escape' })
    expect(screen.queryByRole('menu')).toBeNull()
    fireEvent.contextMenu(screen.getByText('a meeting'))
    fireEvent.keyDown(window, { key: 'ArrowDown' })
    expect(document.activeElement?.textContent).toBe('open')
    fireEvent.mouseDown(document.body)
    expect(screen.queryByRole('menu')).toBeNull()
    expect(open).not.toHaveBeenCalled()
  })
})
