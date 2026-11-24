import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { create } from 'zustand'

/** One line of a right-click menu. A `confirm` line asks once more, under that label,
 * before it does what it says. */
export interface MenuItem {
  label: string
  onSelect: () => void
  danger?: boolean
  confirm?: string
  disabled?: boolean
}

interface MenuState {
  at: { x: number; y: number } | null
  items: MenuItem[]
}

const useMenu = create<MenuState>(() => ({ at: null, items: [] }))

function close() {
  useMenu.setState({ at: null, items: [] })
}

/** What is typed into, where the right click landed in a field. */
type Field = HTMLInputElement | HTMLTextAreaElement | HTMLElement

function fieldOf(target: EventTarget | null): Field | null {
  if (!(target instanceof HTMLElement)) return null
  const field = target.closest('input, textarea, [contenteditable="true"]')
  if (!(field instanceof HTMLElement)) return null
  if (field instanceof HTMLInputElement) {
    const typed = ['text', 'search', 'url', 'email', 'password', 'number', ''].includes(field.type)
    if (!typed) return null
  }
  if ((field as HTMLInputElement).readOnly || (field as HTMLInputElement).disabled) return null
  return field
}

function selectedIn(field: Field | null): string {
  if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) {
    const { selectionStart: start, selectionEnd: end } = field
    return start === null || end === null ? '' : field.value.slice(start, end)
  }
  return window.getSelection()?.toString() ?? ''
}

async function writeClipboard(text: string) {
  try {
    const { writeText } = await import('@tauri-apps/plugin-clipboard-manager')
    await writeText(text)
  } catch {
    await navigator.clipboard?.writeText(text).catch(() => {})
  }
}

async function readClipboard(): Promise<string> {
  try {
    const { readText } = await import('@tauri-apps/plugin-clipboard-manager')
    return (await readText()) ?? ''
  } catch {
    return (await navigator.clipboard?.readText().catch(() => '')) ?? ''
  }
}

/** Cut, copy, paste and select all for a field; copy alone for text chosen elsewhere. */
function editItems(target: EventTarget | null, t: (key: string) => string): MenuItem[] {
  const field = fieldOf(target)
  const chosen = selectedIn(field)
  if (!field) {
    return chosen
      ? [{ label: t('contextMenu.copy'), onSelect: () => void writeClipboard(chosen) }]
      : []
  }
  const typeIn = (text: string) => {
    field.focus()
    // Typed in as by the keyboard, so that undo and the field's own handling still work.
    document.execCommand('insertText', false, text)
  }
  return [
    {
      label: t('contextMenu.cut'),
      disabled: !chosen,
      onSelect: () => {
        void writeClipboard(chosen)
        field.focus()
        document.execCommand('delete')
      },
    },
    {
      label: t('contextMenu.copy'),
      disabled: !chosen,
      onSelect: () => void writeClipboard(chosen),
    },
    {
      label: t('contextMenu.paste'),
      onSelect: () => void readClipboard().then((text) => text && typeIn(text)),
    },
    {
      label: t('contextMenu.selectAll'),
      onSelect: () => {
        field.focus()
        if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement)
          field.select()
        else document.execCommand('selectAll')
      },
    },
  ]
}

/** Opens a right-click menu with `items`, under copy where text is chosen. Call from an
 * element's onContextMenu. */
export function openContextMenu(
  event: {
    clientX: number
    clientY: number
    preventDefault: () => void
    target: EventTarget | null
  },
  items: MenuItem[],
) {
  event.preventDefault()
  const chosen = window.getSelection()?.toString() ?? ''
  const copy: MenuItem[] =
    chosen && !fieldOf(event.target)
      ? [{ label: COPY_LABEL.current, onSelect: () => void writeClipboard(chosen) }]
      : []
  useMenu.setState({ at: { x: event.clientX, y: event.clientY }, items: [...copy, ...items] })
}

/** The label of copy, as the menu host last read it, for menus opened outside React. */
const COPY_LABEL = { current: 'Copy' }

/** Shows the right-click menu, in place of the web view's own: a page's menu where a page
 * gives one, editing where the click lands in a field or on chosen text, and nothing
 * elsewhere. Rendered once per window. */
export function ContextMenuHost() {
  const { t } = useTranslation()
  const { at, items } = useMenu()
  const [asking, setAsking] = useState<number | null>(null)
  const [place, setPlace] = useState<{ x: number; y: number } | null>(null)
  const menu = useRef<HTMLDivElement>(null)
  COPY_LABEL.current = t('contextMenu.copy')

  useEffect(() => {
    const opened = (event: MouseEvent) => {
      // A page that gave its own menu has handled the click.
      if (event.defaultPrevented) return
      event.preventDefault()
      const edit = editItems(event.target, t)
      if (edit.length === 0) return close()
      useMenu.setState({ at: { x: event.clientX, y: event.clientY }, items: edit })
    }
    window.addEventListener('contextmenu', opened)
    return () => window.removeEventListener('contextmenu', opened)
  }, [t])

  useEffect(() => {
    setAsking(null)
    if (!at) return
    const away = (event: Event) => {
      if (menu.current?.contains(event.target as Node)) return
      close()
    }
    // Keys move through the menu without taking focus from the field on open, so that
    // the text chosen in it stays chosen.
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') close()
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
        event.preventDefault()
        const all = [
          ...(menu.current?.querySelectorAll<HTMLButtonElement>(
            '[role="menuitem"]:not(:disabled)',
          ) ?? []),
        ]
        const now = all.indexOf(document.activeElement as HTMLButtonElement)
        const step = event.key === 'ArrowDown' ? 1 : -1
        const next = now < 0 ? (step > 0 ? 0 : all.length - 1) : now + step
        all[(next + all.length) % all.length]?.focus()
      }
    }
    window.addEventListener('mousedown', away, true)
    window.addEventListener('scroll', close, true)
    window.addEventListener('blur', close)
    window.addEventListener('resize', close)
    window.addEventListener('keydown', key)
    return () => {
      window.removeEventListener('mousedown', away, true)
      window.removeEventListener('scroll', close, true)
      window.removeEventListener('blur', close)
      window.removeEventListener('resize', close)
      window.removeEventListener('keydown', key)
    }
  }, [at])

  // Kept inside the window, flipped to the left or above where it would run out of it.
  useLayoutEffect(() => {
    if (!at || !menu.current) return setPlace(null)
    const { width, height } = menu.current.getBoundingClientRect()
    const x = at.x + width > window.innerWidth - 4 ? Math.max(4, at.x - width) : at.x
    const y = at.y + height > window.innerHeight - 4 ? Math.max(4, at.y - height) : at.y
    setPlace({ x, y })
  }, [at, items])

  if (!at || items.length === 0) return null
  return (
    <div
      ref={menu}
      role="menu"
      data-context-menu
      style={{
        left: (place ?? at).x,
        top: (place ?? at).y,
        visibility: place ? 'visible' : 'hidden',
      }}
      className="fixed z-[100] flex min-w-[180px] flex-col rounded-md border border-border bg-bg-elevated py-1 shadow-lg"
    >
      {items.map((item, index) => (
        <button
          key={index}
          type="button"
          role="menuitem"
          disabled={item.disabled}
          // Keeps the field that was right-clicked focused, with its text still chosen.
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => {
            if (item.confirm && asking !== index) return setAsking(index)
            close()
            item.onSelect()
          }}
          className={`cursor-pointer border-none bg-transparent px-3 py-1.5 text-left text-[13px] outline-none hover:bg-bg-secondary focus:bg-bg-secondary disabled:cursor-default disabled:opacity-40 ${
            item.danger ? 'text-error' : 'text-text-primary'
          }`}
        >
          {item.confirm && asking === index ? item.confirm : item.label}
        </button>
      ))}
    </div>
  )
}
