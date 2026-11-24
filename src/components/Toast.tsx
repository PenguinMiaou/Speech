import { useState, useEffect, useCallback } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { CheckCircle2, XCircle, Info, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { timed } from '../lib/animations'
import {
  registerToastHandler,
  registerToastRemover,
  type ToastMessage,
  type ToastType,
} from './toast-service'

const icons: Record<ToastType, typeof Info> = {
  success: CheckCircle2,
  error: XCircle,
  info: Info,
}

const colors: Record<ToastType, string> = {
  success: 'text-success',
  error: 'text-error',
  info: 'text-accent',
}

/** Shows the toasts of this window at the bottom, newest last. An error is announced at
 * once; the others wait for the screen reader to be free. */
export function ToastContainer() {
  const { t } = useTranslation()
  const [toasts, setToasts] = useState<ToastMessage[]>([])

  const remove = useCallback((id: number) => {
    setToasts((prev) => prev.filter((each) => each.id !== id))
  }, [])

  useEffect(() => registerToastRemover(remove), [remove])
  useEffect(() => {
    return registerToastHandler((message) => {
      // At most three at a time; the oldest goes first.
      setToasts((prev) => [...prev.slice(-2), message])
      window.setTimeout(() => remove(message.id), message.ms)
    })
  }, [remove])

  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-5 z-[9999] flex flex-col items-center gap-2 px-4">
      <AnimatePresence initial={false}>
        {toasts.map((each) => {
          const Icon = icons[each.type]
          return (
            <motion.div
              key={each.id}
              layout
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0, transition: timed() }}
              exit={{ opacity: 0, y: 4, transition: timed('fast') }}
              className="pointer-events-auto flex max-w-[420px] items-center gap-2.5 rounded-lg border border-border bg-bg-elevated px-3 py-2 text-[13px] text-text-primary shadow-lg"
              role={each.type === 'error' ? 'alert' : 'status'}
            >
              <Icon size={14} className={`flex-shrink-0 ${colors[each.type]}`} aria-hidden />
              <span className="min-w-0 flex-1">{each.text}</span>
              {each.action && (
                <button
                  type="button"
                  onClick={() => {
                    each.action?.run()
                    remove(each.id)
                  }}
                  className="btn-quiet shrink-0 px-2 text-[13px] font-medium text-accent"
                >
                  {each.action.label}
                </button>
              )}
              <button
                type="button"
                aria-label={t('common.close')}
                onClick={() => remove(each.id)}
                className="shrink-0 cursor-pointer border-none bg-transparent p-0.5 text-text-tertiary hover:text-text-primary"
              >
                <X size={13} />
              </button>
            </motion.div>
          )
        })}
      </AnimatePresence>
    </div>
  )
}
