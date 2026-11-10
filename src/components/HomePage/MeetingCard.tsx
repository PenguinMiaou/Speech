import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { FolderOpen, Mic, Square } from 'lucide-react'
import {
  copyText,
  installSpeakerModels,
  listLocalModels,
  meetingRecordLevel,
  meetingRecordStart,
  meetingRecordStop,
  onFileDropped,
  onFileTranscribeProgress,
  onSpeakerModelsProgress,
  pickMediaFile,
  saveTextFile,
  speakerModelsStatus,
  transcribeFile,
  type FileTranscribeProgress,
  type TranscribeOptions,
} from '../../lib/tauri'
import { useAppStore } from '../../stores/appStore'
import { localModelName } from '../../lib/local-model-names'
import { Toggle } from '../Settings/shared/Toggle'
import { LogoSelect } from '../Settings/shared/LogoSelect'

const KNOWN_ERRORS = [
  'file_not_audio',
  'file_unreadable',
  'file_too_long',
  'local_model_not_installed',
  'speaker_models_not_installed',
  'busy',
]
const SPEAKERS_KEY = 'meeting.speakers'

type Phase = 'idle' | 'recording' | 'working'

function remembered(): boolean {
  try {
    return window.localStorage.getItem(SPEAKERS_KEY) === '1'
  } catch {
    return false
  }
}

function remember(on: boolean) {
  try {
    window.localStorage.setItem(SPEAKERS_KEY, on ? '1' : '0')
  } catch {
    // The choice then lasts until the window closes.
  }
}

function clock(seconds: number): string {
  const two = (value: number) => String(value).padStart(2, '0')
  const hours = Math.floor(seconds / 3600)
  const rest = `${two(Math.floor(seconds / 60) % 60)}:${two(seconds % 60)}`
  return hours > 0 ? `${hours}:${rest}` : rest
}

/** Record a meeting or hand over a recording, and the local model writes it out. */
export function MeetingCard() {
  const { t } = useTranslation()
  const inputDevice = useAppStore((s) => s.config.input_device)
  const dictationModel = useAppStore((s) => s.config.stt_local_model)
  const [phase, setPhase] = useState<Phase>('idle')
  const [progress, setProgress] = useState<FileTranscribeProgress | null>(null)
  const [text, setText] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [level, setLevel] = useState({ seconds: 0, volume: 0 })
  const [speakers, setSpeakers] = useState(false)
  const [speakersInstalled, setSpeakersInstalled] = useState(false)
  const [download, setDownload] = useState<number | null>(null)
  const [models, setModels] = useState<string[]>([])
  const [model, setModel] = useState<string | null>(null)
  const phaseRef = useRef<Phase>('idle')
  const optionsRef = useRef<TranscribeOptions>({ speakerLabel: null, modelId: null })

  optionsRef.current = {
    speakerLabel: speakers && speakersInstalled ? t('meeting.speakerLabel') : null,
    modelId: model,
  }

  const enter = (next: Phase) => {
    phaseRef.current = next
    setPhase(next)
  }

  const finish = (work: Promise<string>) => {
    enter('working')
    setProgress(null)
    setText(null)
    setError(null)
    work
      .then((result) => setText(result ?? ''))
      .catch((reason) => {
        const code = KNOWN_ERRORS.find((known) => String(reason).includes(known))
        setError(`meeting.error.${code ?? 'failed'}`)
      })
      .finally(() => enter('idle'))
  }

  const transcribe = (path: string) => {
    if (phaseRef.current !== 'idle') return
    finish(Promise.resolve(transcribeFile(path, optionsRef.current)))
  }
  const transcribeRef = useRef(transcribe)
  transcribeRef.current = transcribe

  useEffect(() => {
    const dropped = Promise.resolve(
      onFileDropped((paths) => {
        if (paths.length > 0) transcribeRef.current(paths[0])
      }),
    )
    const reported = Promise.resolve(onFileTranscribeProgress(setProgress))
    const downloading = Promise.resolve(
      onSpeakerModelsProgress(({ downloaded, total }) =>
        setDownload(total > 0 ? Math.round((downloaded / total) * 100) : 0),
      ),
    )
    void Promise.resolve(speakerModelsStatus())
      .then((status) => {
        if (!status) return
        setSpeakersInstalled(status.installed)
        setSpeakers(status.installed && remembered())
      })
      .catch(() => {})
    void Promise.resolve(listLocalModels())
      .then((all) =>
        setModels((all ?? []).filter((entry) => entry.installed).map((entry) => entry.id)),
      )
      .catch(() => {})
    return () => {
      void dropped.then((stop) => stop?.())
      void reported.then((stop) => stop?.())
      void downloading.then((stop) => stop?.())
    }
  }, [])

  useEffect(() => {
    if (phase !== 'recording') return
    const read = () =>
      void Promise.resolve(meetingRecordLevel())
        .then((now) => now && setLevel(now))
        .catch(() => {})
    read()
    const timer = window.setInterval(read, 500)
    return () => window.clearInterval(timer)
  }, [phase])

  const toggleSpeakers = (on: boolean) => {
    if (!on || speakersInstalled) {
      setSpeakers(on)
      remember(on)
      return
    }
    if (download !== null) return
    setDownload(0)
    setError(null)
    Promise.resolve(installSpeakerModels())
      .then(() => {
        setSpeakersInstalled(true)
        setSpeakers(true)
        remember(true)
      })
      .catch(() => setError('meeting.error.speakerDownload'))
      .finally(() => setDownload(null))
  }

  const record = () => {
    if (phaseRef.current !== 'idle') return
    setError(null)
    setText(null)
    setLevel({ seconds: 0, volume: 0 })
    enter('recording')
    Promise.resolve(meetingRecordStart(inputDevice)).catch(() => {
      setError('meeting.error.microphone')
      enter('idle')
    })
  }

  const stop = (discard: boolean) => {
    if (phaseRef.current !== 'recording') return
    const done = Promise.resolve(meetingRecordStop({ ...optionsRef.current, discard }))
    if (discard) {
      void done.catch(() => {}).finally(() => enter('idle'))
    } else {
      finish(done)
    }
  }

  const choose = () =>
    void Promise.resolve(pickMediaFile())
      .then((path) => path && transcribe(path))
      .catch(() => {})

  const percent =
    progress && progress.stage === 'text' && progress.total > 0
      ? Math.round((progress.done / progress.total) * 100)
      : null
  const chosenModel = model ?? (models.includes(dictationModel) ? dictationModel : models[0])

  return (
    <section className="surface p-5">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <h3 className="text-[15px] font-semibold">{t('meeting.title')}</h3>
          <p className="mt-1 text-[13px] leading-relaxed text-text-secondary">
            {t('meeting.description')}
          </p>
        </div>
        {phase === 'recording' && (
          <span className="flex items-center gap-2 text-[20px] font-semibold tabular-nums">
            <span
              className="h-2.5 w-2.5 rounded-full bg-error transition-transform"
              style={{ transform: `scale(${1 + Math.min(level.volume, 1) * 1.2})` }}
            />
            {clock(level.seconds)}
          </span>
        )}
      </div>

      {phase === 'idle' && (
        <div className="mt-4 flex flex-wrap items-center gap-2">
          <button type="button" onClick={record} className="btn-record">
            <Mic size={15} />
            {t('meeting.record')}
          </button>
          <button type="button" onClick={choose} className="btn-quiet">
            <FolderOpen size={15} />
            {t('meeting.chooseFile')}
          </button>
        </div>
      )}

      {phase === 'recording' && (
        <div className="mt-4 flex flex-wrap items-center gap-2">
          <button type="button" onClick={() => stop(false)} className="btn-record">
            <Square size={13} fill="currentColor" />
            {t('meeting.stop')}
          </button>
          <button type="button" onClick={() => stop(true)} className="btn-quiet">
            {t('meeting.discard')}
          </button>
        </div>
      )}

      {phase === 'working' && (
        <div className="mt-4">
          <p className="text-[13px] text-text-secondary">
            {progress?.stage === 'speakers'
              ? t('meeting.findingSpeakers')
              : percent !== null && progress
                ? t('meeting.working', { done: progress.done, total: progress.total })
                : t('meeting.reading')}
          </p>
          {percent !== null && (
            <div
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={percent}
              className="mt-2 h-1.5 overflow-hidden rounded-full bg-bg-tertiary"
            >
              <div
                className="h-full rounded-full bg-accent transition-[width] duration-300"
                style={{ width: `${percent}%` }}
              />
            </div>
          )}
        </div>
      )}

      {phase !== 'working' && (
        <div className="mt-4 flex flex-wrap items-center gap-x-6 gap-y-3 border-t border-border pt-4">
          <Toggle
            checked={speakers}
            onChange={toggleSpeakers}
            label={t('meeting.speakers')}
            disabled={phase === 'recording' && !speakersInstalled}
          />
          {download !== null && (
            <span className="text-[12px] text-text-secondary">
              {t('meeting.downloadingSpeakers', { percent: download })}
            </span>
          )}
          {models.length > 1 && chosenModel && (
            <div className="ml-auto w-[190px]">
              <LogoSelect
                compact
                label={t('meeting.model')}
                value={chosenModel}
                options={models.map((id) => ({ value: id, label: localModelName(id) }))}
                onChange={setModel}
              />
            </div>
          )}
        </div>
      )}

      {error && <p className="mt-3 text-[12px] text-error">{t(error)}</p>}

      {text !== null && phase === 'idle' && (
        <div className="mt-4 space-y-2">
          <textarea
            readOnly
            value={text}
            rows={8}
            aria-label={t('meeting.result')}
            className="w-full resize-y rounded-[8px] border border-border bg-bg-primary px-3 py-2.5 text-[13px] leading-relaxed text-text-primary outline-none select-text"
          />
          <div className="flex gap-2">
            <button type="button" onClick={() => void copyText(text)} className="btn-quiet">
              {t('meeting.copy')}
            </button>
            <button
              type="button"
              onClick={() => void saveTextFile(text, t('meeting.fileName'))}
              className="btn-quiet"
            >
              {t('meeting.save')}
            </button>
          </div>
        </div>
      )}
    </section>
  )
}
