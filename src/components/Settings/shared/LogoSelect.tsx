import { Check, ChevronDown, type LucideIcon } from 'lucide-react'
import { useCallback, useId, useRef, useState } from 'react'
import { LlmLogo } from '../LlmLogo'
import { FIELD_SURFACE, OPTION_CLASS, POPOVER_CLASS, useDismiss } from './popover'

export interface LogoOption {
  value: string
  label: string
  /** A logo id, or null for the fallback icon. Left out, the option shows no icon. */
  logo?: string | null
  fallback?: LucideIcon
}

interface Props {
  label: string
  value: string
  options: LogoOption[]
  onChange: (value: string) => void
  /** Shorter, with smaller text, for rows of small controls. */
  compact?: boolean
}

function OptionIcon({ option }: { option: LogoOption | undefined }) {
  if (!option || (option.logo === undefined && !option.fallback)) return null
  return (
    <span data-option-icon className="inline-flex flex-shrink-0">
      <LlmLogo id={option.logo ?? null} fallback={option.fallback} />
    </span>
  )
}

export function LogoSelect({ label, value, options, onChange, compact = false }: Props) {
  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(0)
  const rootRef = useRef<HTMLDivElement>(null)
  const listId = useId()
  const close = useCallback(() => setOpen(false), [])
  useDismiss(rootRef, open, close)

  const chosenIndex = options.findIndex((option) => option.value === value)
  const chosen = options[chosenIndex]

  const show = () => {
    setActive(Math.max(chosenIndex, 0))
    setOpen(true)
  }

  const pick = (next: string) => {
    if (next !== value) onChange(next)
    close()
  }

  const handleKeyDown = (event: React.KeyboardEvent<HTMLButtonElement>) => {
    if (event.key === 'Escape') {
      close()
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      if (!open) {
        show()
        return
      }
      const step = event.key === 'ArrowDown' ? 1 : -1
      setActive((index) => Math.min(Math.max(index + step, 0), options.length - 1))
    } else if (open && (event.key === 'Enter' || event.key === ' ')) {
      event.preventDefault()
      pick(options[active].value)
    }
  }

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        role="combobox"
        aria-label={label}
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-controls={open ? listId : undefined}
        aria-activedescendant={open ? `${listId}-${active}` : undefined}
        onClick={() => (open ? close() : show())}
        onKeyDown={handleKeyDown}
        className={`flex w-full items-center cursor-pointer focus-visible:border-accent ${FIELD_SURFACE} ${
          compact ? 'h-8 gap-2 px-2 text-[12px]' : 'min-h-[40px] gap-2.5 px-2.5 py-2 text-[13px]'
        }`}
      >
        <OptionIcon option={chosen} />
        <span className="min-w-0 flex-1 truncate text-left">{chosen?.label ?? value}</span>
        <ChevronDown size={14} className="flex-shrink-0 text-text-tertiary" />
      </button>
      {open && (
        <div id={listId} role="listbox" aria-label={label} className={POPOVER_CLASS}>
          {options.map((option, index) => (
            <button
              key={option.value}
              id={`${listId}-${index}`}
              type="button"
              role="option"
              aria-selected={option.value === value}
              tabIndex={-1}
              onClick={() => pick(option.value)}
              onMouseEnter={() => setActive(index)}
              className={`${OPTION_CLASS} ${index === active ? 'bg-bg-secondary' : ''}`}
            >
              <OptionIcon option={option} />
              <span className="min-w-0 flex-1 truncate">{option.label}</span>
              {option.value === value && (
                <Check size={14} aria-hidden="true" className="flex-shrink-0 text-accent" />
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
