import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  AudioLines,
  ArrowLeft,
  Copy,
  Download,
  Ellipsis,
  FolderOpen,
  Mic,
  SlidersHorizontal,
  Square,
  Trash2,
} from 'lucide-react'
import {
  copyText,
  forgetSpeaker,
  installSpeakerModels,
  listLocalModels,
  audioSource,
  meetingAudio,
  meetingAudioDelete,
  meetingDelete,
  meetingGet,
  meetingsList,
  meetingRecordLevel,
  meetingRecordStart,
  meetingRecordStop,
  onFileDropped,
  onFileTranscribeProgress,
  onSpeakerModelsProgress,
  pickMediaFile,
  rememberSpeaker,
  saveMeetingAudio,
  saveTextFile,
  speakerModelsStatus,
  speakerVoices,
  transcribeFile,
  type FileTranscribeProgress,
  type MeetingSummary,
  type TranscribeOptions,
} from '../../lib/tauri'
import { useAppStore } from '../../stores/appStore'
import { localModelName } from '../../lib/local-model-names'
import { Toggle } from '../Settings/shared/Toggle'
import { Transcript } from './Transcript'
import { MeetingAudio, type AudioHandle } from './MeetingAudio'
import { TARGET_LANGUAGES } from '../../lib/constants'
import { LogoSelect } from '../Settings/shared/LogoSelect'
import {
  clock,
  meetingText,
  renameSpeakers,
  speakerColor,
  speakersIn,
} from '../../lib/meeting-text'
import {
  listenToMeeting,
  persistMeeting,
  useMeetingStore,
  type MeetingPhase,
} from '../../stores/meetingStore'
import { MeetingMinutes } from './MeetingMinutes'

const KNOWN_ERRORS = [
  'file_not_audio',
  'file_unreadable',
  'file_too_long',
  'local_model_not_installed',
  'speaker_models_not_installed',
  'system_audio_unsupported',
  'busy',
]
const SPEAKERS_KEY = 'meeting.speakers'
const SYSTEM_SOUND_KEY = 'meeting.systemSound'
const TRANSLATE_KEY = 'meeting.translate'
const TRANSLATE_TO_KEY = 'meeting.translateTo'

function rememberedTarget(): string {
  try {
    return window.localStorage.getItem(TRANSLATE_TO_KEY) || 'en'
  } catch {
    return 'en'
  }
}

type Phase = MeetingPhase

function remembered(key = SPEAKERS_KEY): boolean {
  try {
    return window.localStorage.getItem(key) === '1'
  } catch {
    return false
  }
}

function remember(on: boolean, key = SPEAKERS_KEY) {
  try {
    window.localStorage.setItem(key, on ? '1' : '0')
  } catch {
    // The choice then lasts until the window closes.
  }
}

/** Record a meeting or hand over a recording, and the local model writes it out. */
/** A meeting's date and time as the reader's own locale writes them, to the minute. */
function when(time: number): string {
  return new Date(time).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
}

export function MeetingCard() {
  const { t } = useTranslation()
  const inputDevice = useAppStore((s) => s.config.input_device)
  const dictationModel = useAppStore((s) => s.config.stt_local_model)
  const {
    phase,
    text,
    error,
    paragraphs,
    recorded,
    kept,
    notes,
    names,
    minutes,
    id,
    interim,
    title,
  } = useMeetingStore()
  const [past, setPast] = useState<MeetingSummary[]>([])
  const [showOptions, setShowOptions] = useState(false)
  const [showMore, setShowMore] = useState(false)
  const player = useRef<AudioHandle>(null)
  const label = t('meeting.speakerLabel')
  const refreshPast = () =>
    void Promise.resolve(meetingsList())
      .then((all) => setPast(all ?? []))
      .catch(() => {})
  const persist = () => void persistMeeting(label).then(refreshPast)
  const set = useMeetingStore.setState
  const setText = (text: string | null) => set({ text })
  const setError = (error: string | null) => set({ error })
  const setRecorded = (recorded: boolean) => set({ recorded })
  const setKept = (kept: string | null) => set({ kept })
  const [progress, setProgress] = useState<FileTranscribeProgress | null>(null)
  const [level, setLevel] = useState({ seconds: 0, volume: 0, waiting: 0 })
  const liveRef = useRef<HTMLDivElement>(null)
  const [speakers, setSpeakers] = useState(false)
  const [speakersInstalled, setSpeakersInstalled] = useState(false)
  const [systemSound, setSystemSound] = useState(() => remembered(SYSTEM_SOUND_KEY))
  const [translate, setTranslate] = useState(() => remembered(TRANSLATE_KEY))
  const [target, setTarget] = useState(rememberedTarget)
  const [download, setDownload] = useState<number | null>(null)
  const [voices, setVoices] = useState<string[]>([])
  const [tab, setTab] = useState('text')
  const [models, setModels] = useState<string[]>([])
  const [model, setModel] = useState<string | null>(null)
  const optionsRef = useRef<TranscribeOptions>({ speakerLabel: null, modelId: null })

  optionsRef.current = {
    speakerLabel: speakers && speakersInstalled ? t('meeting.speakerLabel') : null,
    modelId: model,
  }

  const enter = (next: Phase) => set({ phase: next })
  const now = () => useMeetingStore.getState().phase

  const finish = (work: Promise<string>) => {
    enter('working')
    setProgress(null)
    set({
      text: null,
      recorded: false,
      error: null,
      names: {},
      minutes: null,
      id: null,
      notes: '',
      voicesAtHand: true,
      title: '',
      duration: 0,
      kept: null,
    })
    work
      .then((result) => {
        setText(result ?? '')
        persist()
      })
      .catch((reason) => {
        const code = KNOWN_ERRORS.find((known) => String(reason).includes(known))
        setError(`meeting.error.${code ?? 'failed'}`)
      })
      .finally(() => enter('idle'))
  }

  const transcribe = (path: string) => {
    if (now() !== 'idle') return
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
    listenToMeeting()
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
    refreshPast()
    void Promise.resolve(speakerVoices())
      .then((all) => setVoices(all ?? []))
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
    if (phase !== 'recording' && phase !== 'finishing') return
    const read = () =>
      void Promise.resolve(meetingRecordLevel())
        .then((now) => now && setLevel(now))
        .catch(() => {})
    read()
    const timer = window.setInterval(read, 500)
    return () => window.clearInterval(timer)
  }, [phase])

  useEffect(() => {
    const live = liveRef.current
    if (live) live.scrollTop = live.scrollHeight
  }, [paragraphs, interim])

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

  const bySpeaker = speakers && speakersInstalled

  const record = () => {
    if (now() !== 'idle') return
    set({
      error: null,
      text: null,
      recorded: false,
      kept: null,
      paragraphs: [],
      notes: '',
      names: {},
      minutes: null,
      id: null,
      voicesAtHand: true,
      title: '',
      duration: 0,
      interim: '',
      translateTo: translate ? target : null,
    })
    setLevel({ seconds: 0, volume: 0, waiting: 0 })
    enter('recording')
    Promise.resolve(meetingRecordStart(inputDevice, bySpeaker, systemSound, model)).catch(
      (reason) => {
        const code = KNOWN_ERRORS.find((known) => String(reason).includes(known))
        setError(`meeting.error.${code ?? 'microphone'}`)
        enter('idle')
      },
    )
  }

  const stop = (discard: boolean) => {
    if (now() !== 'recording') return
    enter('finishing')
    set({ interim: '', duration: level.seconds })
    Promise.resolve(meetingRecordStop(discard))
      // The sound becomes the meeting's own once the meeting is kept; that sets `kept`.
      .then(() => {
        setRecorded(!discard)
        if (!discard) persist()
      })
      .catch(() => setError('meeting.error.failed'))
      .finally(() => enter('idle'))
  }

  const again = () => {
    if (kept) transcribe(kept)
  }

  const forget = () =>
    void Promise.resolve(id ? meetingAudioDelete(id) : undefined)
      .catch(() => {})
      .finally(() => setKept(null))

  const choose = () =>
    void Promise.resolve(pickMediaFile())
      .then((path) => path && transcribe(path))
      .catch(() => {})

  const percent =
    progress && progress.stage === 'text' && progress.total > 0
      ? Math.round((progress.done / progress.total) * 100)
      : null
  const open = (summary: MeetingSummary) =>
    void Promise.resolve(meetingGet(summary.id))
      .then((meeting) => {
        if (!meeting || now() !== 'idle') return
        set({
          id: meeting.id,
          startedAt: meeting.started_at,
          text: meeting.transcript,
          recorded: false,
          paragraphs: [],
          kept: null,
          error: null,
          notes: meeting.notes,
          names: meeting.names ?? {},
          minutes: meeting.minutes || null,
          voicesAtHand: false,
          title: meeting.title ?? '',
          duration: meeting.duration_s ?? 0,
        })
        void Promise.resolve(meetingAudio(meeting.id))
          .then((kept) => useMeetingStore.getState().id === meeting.id && setKept(kept ?? null))
          .catch(() => {})
      })
      .catch(() => {})

  const removePast = (summary: MeetingSummary) =>
    void Promise.resolve(meetingDelete(summary.id))
      .then(() => {
        if (useMeetingStore.getState().id === summary.id) set({ id: null })
        refreshPast()
      })
      .catch(() => {})

  const written = recorded ? meetingText(paragraphs, label) : text
  const speakerNumbers = written ? speakersIn(written, label) : []
  const result = written === null ? null : renameSpeakers(written, label, names)
  const notesBox = (
    <textarea
      value={notes}
      onChange={(event) => set({ notes: event.target.value })}
      onBlur={persist}
      rows={3}
      aria-label={t('meeting.notes')}
      placeholder={t('meeting.notesPlaceholder')}
      className="mt-3 w-full resize-y rounded-[8px] border border-border bg-bg-primary px-3 py-2.5 text-[13px] leading-relaxed text-text-primary outline-none placeholder:text-text-tertiary"
    />
  )
  const chosenModel = model ?? (models.includes(dictationModel) ? dictationModel : models[0])

  const close = () =>
    set({
      text: null,
      recorded: false,
      paragraphs: [],
      id: null,
      minutes: null,
      notes: '',
      names: {},
      title: '',
      duration: 0,
      kept: null,
    })
  const removeThis = () => {
    const open = id
    close()
    if (open)
      void Promise.resolve(meetingDelete(open))
        .then(refreshPast)
        .catch(() => {})
  }
  const busy = phase === 'recording' || phase === 'finishing'
  const detail = phase === 'idle' && result !== null
  const list = phase === 'idle' && result === null
  const startedAt = useMeetingStore.getState().startedAt
  const duration = useMeetingStore.getState().duration
  const item =
    'cursor-pointer border-none bg-transparent px-3 py-1.5 text-left text-[13px] text-text-primary hover:bg-bg-secondary'
  return (
    <section className="flex flex-col gap-5">
      {detail && (
        <div>
          <button
            type="button"
            onClick={close}
            aria-label={t('meeting.all')}
            title={t('meeting.all')}
            className="btn-quiet px-2.5"
          >
            <ArrowLeft size={16} />
          </button>
        </div>
      )}
      <header className="flex items-end justify-between gap-4">
        <div className="min-w-0 flex-1">
          {detail ? (
            <>
              <input
                id="meeting-title"
                value={title}
                onChange={(event) => set({ title: event.target.value })}
                onBlur={persist}
                aria-label={t('meeting.titlePlaceholder')}
                placeholder={startedAt > 0 ? when(startedAt) : t('meeting.titlePlaceholder')}
                className="page-title w-full border-none bg-transparent p-0 outline-none placeholder:text-text-primary"
              />
              <p className="m-0 mt-1 flex flex-wrap gap-x-4 text-[12px] tabular-nums text-text-secondary">
                {startedAt > 0 && <span>{when(startedAt)}</span>}
                {duration > 0 && <span>{clock(duration)}</span>}
                {speakerNumbers.length > 0 && (
                  <span>{t('meeting.people', { count: speakerNumbers.length })}</span>
                )}
              </p>
            </>
          ) : (
            <h2 className="page-title">{t('meeting.title')}</h2>
          )}
        </div>
        {list && (
          <div className="flex shrink-0 items-center gap-2">
            <button
              type="button"
              aria-expanded={showOptions}
              onClick={() => setShowOptions(!showOptions)}
              aria-label={t('meeting.options')}
              title={t('meeting.options')}
              className="btn-quiet px-2.5"
            >
              <SlidersHorizontal size={16} />
            </button>
            <button
              type="button"
              onClick={choose}
              aria-label={t('meeting.chooseFile')}
              title={t('meeting.chooseFile')}
              className="btn-quiet px-2.5"
            >
              <FolderOpen size={16} />
            </button>
            <button type="button" onClick={record} className="btn-record">
              <Mic size={15} />
              {t('meeting.record')}
            </button>
          </div>
        )}
        {busy && (
          <div className="flex flex-wrap items-center gap-3">
            <span className="flex items-center gap-2 text-[20px] font-semibold tabular-nums">
              <span
                className="h-2.5 w-2.5 rounded-full bg-error transition-transform"
                style={{ transform: `scale(${1 + Math.min(level.volume, 1) * 1.2})` }}
              />
              {clock(level.seconds)}
            </span>
            {phase === 'recording' && (
              <>
                <button
                  type="button"
                  onClick={() => stop(true)}
                  aria-label={t('meeting.discard')}
                  title={t('meeting.discard')}
                  className="btn-quiet px-2.5"
                >
                  <Trash2 size={16} />
                </button>
                <button type="button" onClick={() => stop(false)} className="btn-record">
                  <Square size={13} fill="currentColor" />
                  {t('meeting.stop')}
                </button>
              </>
            )}
          </div>
        )}
        {detail && (
          <div className="relative flex shrink-0 items-center gap-2">
            <button
              type="button"
              onClick={() => void copyText(result ?? '')}
              aria-label={t('meeting.copy')}
              title={t('meeting.copy')}
              className="btn-quiet px-2.5"
            >
              <Copy size={16} />
            </button>
            <button
              type="button"
              onClick={() => void saveTextFile(result ?? '', t('meeting.fileName'))}
              aria-label={t('meeting.save')}
              title={t('meeting.save')}
              className="btn-quiet px-2.5"
            >
              <Download size={16} />
            </button>
            <button
              type="button"
              aria-label={t('meeting.more')}
              title={t('meeting.more')}
              aria-expanded={showMore}
              onClick={() => setShowMore(!showMore)}
              className="btn-quiet px-2.5"
            >
              <Ellipsis size={16} />
            </button>
            {showMore && (
              <div
                role="menu"
                onClick={() => setShowMore(false)}
                className="absolute right-0 top-full z-20 mt-1 flex min-w-[168px] flex-col rounded-[8px] border border-border bg-bg-elevated py-1 shadow-lg"
              >
                {kept && id && (
                  <>
                    <button
                      type="button"
                      role="menuitem"
                      onClick={() => void saveMeetingAudio(id, t('meeting.audioFileName'))}
                      className={item}
                    >
                      {t('meeting.saveAudio')}
                    </button>
                    <button type="button" role="menuitem" onClick={again} className={item}>
                      {t('meeting.again')}
                    </button>
                    <button type="button" role="menuitem" onClick={forget} className={item}>
                      {t('meeting.deleteAudio')}
                    </button>
                  </>
                )}
                <button
                  type="button"
                  role="menuitem"
                  onClick={removeThis}
                  className={`${item} text-error`}
                >
                  {t('meeting.deleteThis')}
                </button>
              </div>
            )}
          </div>
        )}
      </header>

      {list && showOptions && (
        <div className="flex flex-col gap-4 border-y border-border py-4">
          <div className="flex flex-wrap items-center gap-x-6 gap-y-3">
            <Toggle checked={speakers} onChange={toggleSpeakers} label={t('meeting.speakers')} />
            <Toggle
              checked={systemSound}
              onChange={(on) => {
                setSystemSound(on)
                remember(on, SYSTEM_SOUND_KEY)
              }}
              label={t('meeting.systemSound')}
            />
            <Toggle
              checked={translate}
              onChange={(on) => {
                setTranslate(on)
                remember(on, TRANSLATE_KEY)
              }}
              label={t('meeting.translate')}
            />
            {translate && (
              <select
                id="meeting-translate-to"
                aria-label={t('meeting.translateTo')}
                value={target}
                onChange={(event) => {
                  setTarget(event.target.value)
                  try {
                    window.localStorage.setItem(TRANSLATE_TO_KEY, event.target.value)
                  } catch {
                    // The choice then lasts for this window only.
                  }
                }}
                className="h-8 rounded-[8px] border border-border bg-bg-primary px-2 text-[12px] text-text-primary outline-none"
              >
                {TARGET_LANGUAGES.map((language) => (
                  <option key={language.value} value={language.value}>
                    {language.label}
                  </option>
                ))}
              </select>
            )}
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
          {voices.length > 0 && (
            <div className="flex flex-wrap items-center gap-2 border-t border-border pt-3">
              <span className="text-[12px] text-text-secondary">
                {t('meeting.voices', { count: voices.length })}
              </span>
              <ul
                className="m-0 flex list-none flex-wrap gap-2 p-0"
                aria-label={t('meeting.voicesList')}
              >
                {voices.map((name) => (
                  <li
                    key={name}
                    className="flex items-center gap-1.5 rounded-[8px] border border-border px-2.5 py-1 text-[12px]"
                  >
                    {name}
                    <button
                      type="button"
                      aria-label={t('meeting.forgetVoice', { name })}
                      onClick={() =>
                        void Promise.resolve(forgetSpeaker(name))
                          .then((all) => setVoices(all ?? []))
                          .catch(() => {})
                      }
                      className="cursor-pointer border-none bg-transparent p-0 text-text-tertiary hover:text-error"
                    >
                      ×
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}

      {busy && (
        <div className="flex flex-col border-t border-border pt-4">
          <div
            ref={liveRef}
            role="log"
            aria-label={t('meeting.live')}
            className="h-[38vh] min-h-[180px] space-y-3 overflow-y-auto text-[13px] leading-relaxed"
          >
            {paragraphs.length === 0 && !interim && (
              <p className="text-text-tertiary">{t('meeting.listening')}</p>
            )}
            {paragraphs.map((paragraph, index) => {
              const who = paragraph.speaker
              return (
                <div key={index}>
                  {who !== null && (
                    <p className="text-[12px]">
                      <span className="font-medium" style={{ color: speakerColor(who) }}>
                        {names[who]?.trim() || (who > 0 ? `${label} ${who}` : label)}
                      </span>
                      <span className="tabular-nums text-text-tertiary">
                        {`  ${clock(Math.floor(paragraph.startMs / 1000))}`}
                      </span>
                    </p>
                  )}
                  <p>{paragraph.text}</p>
                  {paragraph.translation && (
                    <p className="text-text-secondary">{paragraph.translation}</p>
                  )}
                </div>
              )
            })}
            {interim && <p className="text-text-tertiary">{interim}</p>}
          </div>
          {notesBox}
          {phase === 'finishing' && (
            <p className="mt-3 text-[13px] text-text-secondary">
              {t('meeting.finishing', { seconds: level.waiting })}
            </p>
          )}
        </div>
      )}

      {phase === 'working' && (
        <div className="border-t border-border pt-4">
          <div>
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
        </div>
      )}

      {error && <p className="m-0 text-[12px] text-error">{t(error)}</p>}

      {list &&
        (past.length > 0 ? (
          <ul
            className="m-0 list-none divide-y divide-border border-y border-border p-0"
            aria-label={t('meeting.pastList')}
          >
            {past.map((summary) => (
              <li key={summary.id} className="group flex items-center gap-3 py-3">
                <button
                  type="button"
                  onClick={() => open(summary)}
                  className="flex min-w-0 flex-1 cursor-pointer flex-col gap-0.5 border-none bg-transparent p-0 text-left"
                >
                  <span className="truncate text-[13px] font-medium text-text-primary">
                    {summary.title?.trim() || summary.preview}
                  </span>
                  <span className="text-[12px] tabular-nums text-text-tertiary">
                    {when(summary.started_at)}
                    {(summary.duration_s ?? 0) > 0 && (
                      <span className="ml-4">{clock(summary.duration_s ?? 0)}</span>
                    )}
                  </span>
                </button>
                {summary.has_minutes && (
                  <span className="text-[12px] text-text-tertiary">{t('meeting.tabMinutes')}</span>
                )}
                <button
                  type="button"
                  aria-label={t('meeting.deleteMeeting')}
                  onClick={() => removePast(summary)}
                  className="cursor-pointer border-none bg-transparent p-1 text-text-tertiary opacity-0 transition-opacity hover:text-error focus-visible:opacity-100 group-hover:opacity-100"
                >
                  <Trash2 size={14} />
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <div className="flex flex-col items-center gap-2 border-y border-border py-14 text-center">
            <AudioLines size={22} className="text-text-tertiary" />
            <p className="m-0 max-w-[320px] text-[13px] leading-relaxed text-text-secondary">
              {t('meeting.empty')}
            </p>
          </div>
        ))}

      {detail && result !== null && (
        <>
          <div role="tablist" className="-mb-1 flex gap-6 border-b border-border">
            {(['text', 'minutes'] as const).map((each) => (
              <button
                key={each}
                type="button"
                role="tab"
                aria-selected={tab === each}
                onClick={() => setTab(each)}
                className={`-mb-px cursor-pointer border-0 border-b-2 border-solid bg-transparent px-0 pb-2 text-[13px] ${
                  tab === each
                    ? 'border-text-primary font-medium text-text-primary'
                    : 'border-transparent text-text-secondary hover:text-text-primary'
                }`}
              >
                {t(each === 'text' ? 'meeting.tabText' : 'meeting.tabMinutes')}
              </button>
            ))}
          </div>
          <div hidden={tab !== 'text'} className="flex flex-col gap-4">
            {kept && <MeetingAudio ref={player} src={audioSource(kept)} />}
            {speakerNumbers.length > 0 && (
              <div className="flex flex-wrap gap-2">
                {speakerNumbers.map((number) => (
                  <input
                    key={number}
                    value={names[number] ?? ''}
                    onChange={(event) => set({ names: { ...names, [number]: event.target.value } })}
                    onBlur={(event) =>
                      useMeetingStore.getState().voicesAtHand &&
                      void Promise.resolve(rememberSpeaker(number, event.target.value))
                        .then((all) => all && setVoices(all))
                        .catch(() => {})
                        .finally(persist)
                    }
                    aria-label={`${label} ${number}`}
                    placeholder={`${label} ${number}`}
                    style={{ color: speakerColor(number) }}
                    className="h-8 w-[130px] rounded-[8px] border border-border bg-bg-primary px-2.5 text-[12px] font-medium outline-none placeholder:font-normal placeholder:text-text-tertiary"
                  />
                ))}
              </div>
            )}
            <Transcript
              text={written ?? ''}
              names={names}
              speakerLabel={label}
              label={t('meeting.result')}
              onSeek={kept ? (seconds) => player.current?.seek(seconds) : undefined}
            />
          </div>
          <div hidden={tab !== 'minutes'}>
            {notesBox}
            {result.trim() !== '' && (
              <MeetingMinutes
                key={id ?? 'new'}
                transcript={result}
                notes={notes}
                minutes={minutes}
                onMinutes={(written) => {
                  set({ minutes: written })
                  persist()
                }}
              />
            )}
          </div>
        </>
      )}
    </section>
  )
}
