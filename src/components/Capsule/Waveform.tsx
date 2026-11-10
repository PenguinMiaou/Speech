import { useEffect, useRef } from 'react'
import { useReducedMotion } from 'framer-motion'
import { useAppStore } from '../../stores/appStore'

const BAR_COUNT = 7
const MIN_HEIGHT = 3
const MAX_HEIGHT = 16

interface Props {
  bars?: number
  maxHeight?: number
  barClassName?: string
  /** Moves on its own instead of following the microphone. */
  idle?: boolean
}

export function Waveform({
  bars = BAR_COUNT,
  maxHeight = MAX_HEIGHT,
  barClassName = 'bg-white/80',
  idle = false,
}: Props) {
  const barsRef = useRef<(HTMLDivElement | null)[]>([])
  const rafRef = useRef<number>(0)
  const reduced = useReducedMotion()

  useEffect(() => {
    if (reduced) {
      // Static bars at mid-height when reduced motion is preferred
      barsRef.current.forEach((bar) => {
        if (!bar) return
        bar.style.height = `${(MIN_HEIGHT + maxHeight) / 2}px`
        bar.style.opacity = '0.7'
      })
      return
    }

    const animate = () => {
      const volume = idle ? 0.3 : useAppStore.getState().audioVolume
      barsRef.current.forEach((bar, i) => {
        if (!bar) return
        const offset = Math.sin(Date.now() / (idle ? 420 : 200) + i * 0.9) * 0.15
        // A long row rises toward its middle, so it reads as one voice and not as separate meters.
        const shape = bars > BAR_COUNT ? 0.35 + 0.65 * Math.sin((Math.PI * i) / (bars - 1)) : 1
        const normalized = Math.max(0, Math.min(1, volume * shape + offset))
        const height = MIN_HEIGHT + (maxHeight - MIN_HEIGHT) * normalized
        const opacity = Math.max(0.5, normalized)
        bar.style.height = `${height}px`
        bar.style.opacity = `${opacity}`
      })
      rafRef.current = requestAnimationFrame(animate)
    }

    rafRef.current = requestAnimationFrame(animate)
    return () => cancelAnimationFrame(rafRef.current)
  }, [bars, idle, maxHeight, reduced])

  return (
    <div
      className="flex items-center justify-center gap-[3px]"
      style={{ height: `${maxHeight}px` }}
    >
      {Array.from({ length: bars }).map((_, i) => (
        <div
          key={i}
          ref={(el) => {
            barsRef.current[i] = el
          }}
          className={`w-[2px] rounded-full ${barClassName}`}
          style={{
            height: `${MIN_HEIGHT}px`,
            opacity: 0.5,
            transition: 'height 75ms ease-out, opacity 75ms ease-out',
          }}
        />
      ))}
    </div>
  )
}
