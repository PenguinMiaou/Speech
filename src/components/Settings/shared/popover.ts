import { useEffect, type RefObject } from 'react'

/** A field's look without its text size. */
export const FIELD_SURFACE =
  'bg-bg-secondary border border-border rounded-[10px] text-text-primary placeholder:text-text-tertiary outline-none focus:border-accent transition-colors'
export const FIELD_CLASS = `${FIELD_SURFACE} text-[13px]`
export const POPOVER_CLASS =
  'absolute left-0 right-0 z-20 mt-1 max-h-[300px] overflow-y-auto rounded-[10px] border border-border bg-bg-primary p-1 shadow-lg'
export const OPTION_CLASS =
  'flex w-full items-center gap-2.5 rounded-[8px] border-none bg-transparent px-2 py-1.5 text-left text-[13px] text-text-primary cursor-pointer hover:bg-bg-secondary'

export function useDismiss(ref: RefObject<HTMLElement | null>, open: boolean, close: () => void) {
  useEffect(() => {
    if (!open) return
    const onPointerDown = (event: MouseEvent) => {
      if (!ref.current?.contains(event.target as Node)) close()
    }
    document.addEventListener('mousedown', onPointerDown)
    return () => document.removeEventListener('mousedown', onPointerDown)
  }, [close, open, ref])
}
