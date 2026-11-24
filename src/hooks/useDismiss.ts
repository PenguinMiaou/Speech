import { useEffect, type RefObject } from 'react'

/** Closes something open, such as a menu, on Escape, and on a press outside `inside` unless
 * `outside` is false. A press in the right-click menu is not outside: it acts on what was
 * right-clicked in it. */
export function useDismiss(
  open: boolean,
  close: () => void,
  inside: RefObject<HTMLElement | null>,
  outside = true,
) {
  useEffect(() => {
    if (!open) return
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') close()
    }
    const press = (event: MouseEvent) => {
      const target = event.target as Node
      if ((target as HTMLElement).closest?.('[data-context-menu]')) return
      if (inside.current && !inside.current.contains(target)) close()
    }
    window.addEventListener('keydown', key)
    if (outside) window.addEventListener('mousedown', press, true)
    return () => {
      window.removeEventListener('keydown', key)
      window.removeEventListener('mousedown', press, true)
    }
  }, [open, close, inside, outside])
}
