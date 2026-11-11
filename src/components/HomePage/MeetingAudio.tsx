import { forwardRef, useImperativeHandle, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Pause, Play } from 'lucide-react'
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

/** Plays a meeting's kept sound: play or pause, where it is, and a bar to move along it. */
export const MeetingAudio = forwardRef<AudioHandle, Props>(function MeetingAudio(
  { src, onPosition },
  ref,
) {
  const { t } = useTranslation()
  const audio = useRef<HTMLAudioElement>(null)
  const [playing, setPlaying] = useState(false)
  const [at, setPosition] = useState(0)
  const [length, setLength] = useState(0)
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
        onLoadedMetadata={(event) =>
          setLength(
            Number.isFinite(event.currentTarget.duration) ? event.currentTarget.duration : 0,
          )
        }
      />
      <button
        type="button"
        onClick={toggle}
        aria-label={t(playing ? 'meeting.pause' : 'meeting.play')}
        className="flex h-8 w-8 flex-none cursor-pointer items-center justify-center rounded-full border-none bg-text-primary text-bg-primary"
      >
        {playing ? <Pause size={14} fill="currentColor" /> : <Play size={14} fill="currentColor" />}
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
    </div>
  )
})
