import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Pause, Play, RotateCcw, RotateCw } from 'lucide-react'
import { clock } from '../../lib/meeting-text'

export interface AudioHandle {
  /** Moves to this many seconds into the meeting, and plays from there unless told not to. */
  seek: (seconds: number, play?: boolean) => void
}

interface Props {
  src: string
  /** Told where the sound is, in seconds, whenever that changes. */
  onPosition?: (seconds: number) => void
}

/** The speeds the sound can be played at, in the order a click goes through them. */
const RATES = [1, 1.25, 1.5, 2, 0.75]
const RATE_KEY = 'meeting.rate'
/** How far the buttons and arrow keys move back and on, in seconds. */
const BACK = 5
const ON = 15

/** Whether a key pressed now is meant for what has focus, such as a field being typed in or
 * a button that Space presses, and not for the player. */
function typing(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  return Boolean(
    target.closest(
      'input, textarea, select, button, a, [contenteditable="true"], [role="menuitem"], [role="tab"], [role="dialog"]',
    ),
  )
}

/** Plays a meeting's kept sound: play or pause, back and on, the speed, where it is, and a
 * bar to move along it. Space plays or pauses and the arrow keys move back and on, where no
 * field or button has the keyboard. */
export const MeetingAudio = forwardRef<AudioHandle, Props>(function MeetingAudio(
  { src, onPosition },
  ref,
) {
  const { t } = useTranslation()
  const audio = useRef<HTMLAudioElement>(null)
  const [playing, setPlaying] = useState(false)
  const [at, setPosition] = useState(0)
  const [length, setLength] = useState(0)
  const [rate, setRate] = useState(() => {
    try {
      const kept = Number(window.localStorage.getItem(RATE_KEY))
      return RATES.includes(kept) ? kept : 1
    } catch {
      return 1
    }
  })
  /** Where the sound was, and whether it played, when the same sound came to be read from
   * another file (once packed): it goes on from there. */
  const resume = useRef<{ at: number; playing: boolean } | null>(null)
  const shown = useRef(src)
  if (shown.current !== src) {
    const player = audio.current
    resume.current = player ? { at: player.currentTime, playing: !player.paused } : null
    shown.current = src
  }
  const setAt = (seconds: number) => {
    setPosition(seconds)
    onPosition?.(seconds)
  }

  useImperativeHandle(ref, () => ({
    seek: (seconds, play = true) => {
      const player = audio.current
      if (!player) return
      player.currentTime = seconds
      setAt(seconds)
      if (play) void Promise.resolve(player.play()).catch(() => {})
    },
  }))

  const toggle = () => {
    const player = audio.current
    if (!player) return
    if (player.paused) void Promise.resolve(player.play()).catch(() => {})
    else player.pause()
  }
  const skip = (seconds: number) => {
    const player = audio.current
    if (!player) return
    const end = Number.isFinite(player.duration) ? player.duration : Infinity
    const next = Math.max(0, Math.min(end, player.currentTime + seconds))
    player.currentTime = next
    setAt(next)
  }
  const nextRate = () => {
    const next = RATES[(RATES.indexOf(rate) + 1) % RATES.length]
    setRate(next)
    try {
      window.localStorage.setItem(RATE_KEY, String(next))
    } catch {
      // The speed then lasts only for this sound.
    }
  }
  useEffect(() => {
    if (audio.current) audio.current.playbackRate = rate
  }, [rate, src])
  const keys = useRef({ toggle, skip })
  keys.current = { toggle, skip }
  useEffect(() => {
    const press = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return
      if (typing(event.target)) return
      if (event.key === ' ') keys.current.toggle()
      else if (event.key === 'ArrowLeft') keys.current.skip(-BACK)
      else if (event.key === 'ArrowRight') keys.current.skip(ON)
      else return
      event.preventDefault()
    }
    window.addEventListener('keydown', press)
    return () => window.removeEventListener('keydown', press)
  }, [])
  const quiet =
    'flex h-7 flex-none cursor-pointer items-center justify-center gap-0.5 rounded-full border-none bg-transparent px-1.5 text-[11px] tabular-nums text-text-secondary hover:bg-bg-secondary hover:text-text-primary'

  return (
    <div className="flex items-center gap-3">
      <audio
        ref={audio}
        src={src}
        preload="metadata"
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onEnded={() => setPlaying(false)}
        onTimeUpdate={(event) => setAt(event.currentTarget.currentTime)}
        onLoadedMetadata={(event) => {
          const player = event.currentTarget
          player.playbackRate = rate
          setLength(Number.isFinite(player.duration) ? player.duration : 0)
          const from = resume.current
          resume.current = null
          if (!from) return
          player.currentTime = from.at
          if (from.playing) void Promise.resolve(player.play()).catch(() => {})
        }}
      />
      <button
        type="button"
        onClick={toggle}
        aria-label={t(playing ? 'meeting.pause' : 'meeting.play')}
        className="flex h-8 w-8 flex-none cursor-pointer items-center justify-center rounded-full border-none bg-text-primary text-bg-primary"
      >
        {playing ? <Pause size={14} fill="currentColor" /> : <Play size={14} fill="currentColor" />}
      </button>
      <button
        type="button"
        onClick={() => skip(-BACK)}
        aria-label={t('meeting.skipBack', { seconds: BACK })}
        title={t('meeting.skipBack', { seconds: BACK })}
        className={quiet}
      >
        <RotateCcw size={13} aria-hidden />
        {BACK}
      </button>
      <button
        type="button"
        onClick={() => skip(ON)}
        aria-label={t('meeting.skipOn', { seconds: ON })}
        title={t('meeting.skipOn', { seconds: ON })}
        className={quiet}
      >
        {ON}
        <RotateCw size={13} aria-hidden />
      </button>
      <input
        id="meeting-audio-position"
        type="range"
        min={0}
        max={length || 0}
        step={1}
        value={Math.min(at, length || at)}
        onChange={(event) => {
          const seconds = Number(event.target.value)
          if (audio.current) audio.current.currentTime = seconds
          setAt(seconds)
        }}
        aria-label={t('meeting.position')}
        className="meeting-audio-bar min-w-0 flex-1"
      />
      <span className="flex-none text-[12px] tabular-nums text-text-secondary">
        {clock(Math.floor(at))} / {clock(Math.floor(length))}
      </span>
      <button
        type="button"
        onClick={nextRate}
        aria-label={t('meeting.rate', { rate })}
        title={t('meeting.rate', { rate })}
        className={`${quiet} min-w-[38px]`}
      >
        {rate}×
      </button>
    </div>
  )
})
