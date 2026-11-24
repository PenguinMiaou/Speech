export type ToastType = 'success' | 'error' | 'info'

/** A button a toast carries, such as Undo. */
export interface ToastAction {
  label: string
  run: () => void
}

export interface ToastMessage {
  id: number
  text: string
  type: ToastType
  /** How long it stays, in milliseconds. */
  ms: number
  action?: ToastAction
}

type ToastHandler = (message: ToastMessage) => void

let addToast: ToastHandler = () => {}
let removeToast: (id: number) => void = () => {}

/** Lets the toasts in view be taken away before their time. */
export function registerToastRemover(remover: (id: number) => void) {
  removeToast = remover
  return () => {
    if (removeToast === remover) removeToast = () => {}
  }
}
let next = 1

/** How long each kind of toast stays when no length is given: an error is read longer. */
const STAYS: Record<ToastType, number> = { success: 3000, info: 3500, error: 6000 }

export function registerToastHandler(handler: ToastHandler) {
  addToast = handler
  return () => {
    if (addToast === handler) addToast = () => {}
  }
}

export function toast(
  text: string,
  type: ToastType = 'info',
  options: { ms?: number; action?: ToastAction } = {},
): number {
  const id = next++
  addToast({ id, text, type, ms: options.ms ?? STAYS[type], action: options.action })
  return id
}

toast.success = (text: string) => toast(text, 'success')
toast.error = (text: string) => toast(text, 'error')

/** How long a change that can be undone waits before it is made for good. */
export const UNDO_MS = 7000

/** Says that something was done, with a way to undo it for a while. `commit` makes the
 * change for good once the time is up, unless `undo` was chosen first; exactly one of
 * them runs. Returns a function that commits at once, for when the change must not wait
 * any longer. */
toast.undoable = (
  text: string,
  {
    label,
    undo,
    commit,
    ms = UNDO_MS,
  }: { label: string; undo: () => void; commit: () => void; ms?: number },
): (() => void) => {
  let settled = false
  const settle = (then: () => void) => {
    if (settled) return
    settled = true
    window.clearTimeout(timer)
    then()
  }
  const timer = window.setTimeout(() => settle(commit), ms)
  const id = toast(text, 'info', { ms, action: { label, run: () => settle(undo) } })
  return () => {
    removeToast(id)
    settle(commit)
  }
}
