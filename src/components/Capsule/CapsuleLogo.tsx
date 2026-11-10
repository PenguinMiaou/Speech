import { useId } from 'react'

export function CapsuleLogo({ size = 22, className = '' }: { size?: number; className?: string }) {
  const id = useId()
  const bodyId = `mic-body-${id}`
  const highlightId = `mic-highlight-${id}`

  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" className={className}>
      <defs>
        <linearGradient id={bodyId} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="currentColor" stopOpacity="1" />
          <stop offset="100%" stopColor="currentColor" stopOpacity="0.8" />
        </linearGradient>
        <linearGradient id={highlightId} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="currentColor" stopOpacity="0.9" />
          <stop offset="100%" stopColor="currentColor" stopOpacity="0.3" />
        </linearGradient>
      </defs>

      {/* Mic head */}
      <rect x="8.5" y="3" width="7" height="11" rx="3.5" fill={`url(#${bodyId})`} />

      {/* Inner highlight */}
      <ellipse cx="10.5" cy="6.5" rx="1.5" ry="2.5" fill={`url(#${highlightId})`} />

      {/* Arc stand */}
      <path
        d="M7 13.5a5 5 0 0 0 10 0"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        fill="none"
        opacity="0.9"
      />

      {/* Stem */}
      <line
        x1="12"
        y1="18.5"
        x2="12"
        y2="21"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        opacity="0.9"
      />

      {/* Base */}
      <line
        x1="9.5"
        y1="21"
        x2="14.5"
        y2="21"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        opacity="0.9"
      />
    </svg>
  )
}
