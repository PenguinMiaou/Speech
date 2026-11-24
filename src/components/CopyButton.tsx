import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Check, Copy } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { copyText } from '../lib/tauri'
import { toast } from './toast-service'

interface Props {
  /** What is copied, or how to make it at the moment of copying. */
  text: string | (() => string)
  /** The button's name, read out and shown on hover. */
  label: string
  size?: number
  className?: string
  /** Words shown beside the icon; an icon alone where none are given. */
  children?: ReactNode
}

/** A copy button that answers: a tick for a moment once the text is on the clipboard, and
 * an error where it could not be put there. The same everywhere it is used. */
export function CopyButton({
  text,
  label,
  size = 15,
  className = 'btn-quiet px-2.5',
  children,
}: Props) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)
  const timer = useRef<number | undefined>(undefined)
  useEffect(() => () => window.clearTimeout(timer.current), [])
  const Icon = copied ? Check : Copy
  return (
    <button
      type="button"
      aria-label={copied ? t('common.copied') : label}
      title={copied ? t('common.copied') : label}
      onClick={() =>
        void Promise.resolve(copyText(typeof text === 'function' ? text() : text))
          .then(() => {
            setCopied(true)
            window.clearTimeout(timer.current)
            timer.current = window.setTimeout(() => setCopied(false), 1500)
          })
          .catch(() => toast.error(t('common.copyFailed')))
      }
      className={className}
    >
      <Icon size={size} className={copied ? 'text-success' : undefined} aria-hidden />
      {children}
      <span aria-live="polite" className="sr-only">
        {copied ? t('common.copied') : ''}
      </span>
    </button>
  )
}
