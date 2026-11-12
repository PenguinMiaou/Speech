import { Fragment, useEffect, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import {
  AudioLines,
  ArrowLeft,
  Cloud,
  Copy,
  Download,
  Files,
  Ellipsis,
  Folder,
  FolderInput,
  FolderOpen,
  Mic,
  Pause,
  Play,
  SlidersHorizontal,
  Check,
  FileAudio,
  Hourglass,
  SpellCheck,
  Diff,
  Type,
  Sparkles,
  Square,
  Trash2,
  Users,
  Search,
  MessageCircleQuestion,
  LayoutTemplate,
  ScrollText,
} from 'lucide-react'
import {
  addDictionaryEntry,
  copyText,
  minutesTemplates,
  type MinutesTemplate,
  forgetSpeaker,
  installSpeakerModels,
  installWordTimes,
  listLocalModels,
  audioSource,
  meetingAudio,
  meetingAudioDelete,
  meetingDelete,
  meetingFolder,
  meetingGet,
  meetingLanguages,
  meetingsAsk,
  meetingsList,
  meetingsSearch,
  meetingRecordLevel,
  meetingRecordPause,
  meetingRecordStart,
  meetingRecordStop,
  onFileDropped,
  onSpeakerModelsProgress,
  onWordTimesProgress,
  pickMediaFile,
  rememberSpeaker,
  speakerFewer,
  saveMeetingAudio,
  saveTextFile,
  speakerModelsStatus,
  wordTimesStatus,
  speakerVoices,
  transcribeFile,
  transcribeFileStop,
  type MeetingSummary,
  type TranscribeOptions,
} from '../../lib/tauri'
import { useAppStore } from '../../stores/appStore'
import { localModelName } from '../../lib/local-model-names'
import { plainOf } from '../../lib/meeting-format'
import { BUILT_IN, TemplateGallery, templateText } from './TemplateGallery'
import { MeetingTerms } from './MeetingTerms'
import { Toggle } from '../Settings/shared/Toggle'
import { Transcript } from './Transcript'
import { MeetingAudio, type AudioHandle } from './MeetingAudio'
import { TARGET_LANGUAGES } from '../../lib/constants'
import { LogoSelect } from '../Settings/shared/LogoSelect'
import {
  clock,
  remaining,
  meetingText,
  subtitles,
  timesOf,
  renameSpeakers,
  saidBy,
  speakerColor,
  speakersIn,
  stampNote,
  stampedNotes,
  withLines,
  withRunsJoined,
  withSpeakerAt,
  withSpeakersJoined,
  withSpellings,
} from '../../lib/meeting-text'
import {
  editParagraph,
  learnFrom,
  makeMinutes,
  listenToMeeting,
  nameFromMinutes,
  nameMeeting,
  recoverMeeting,
  timeWords,
  persistMeeting,
  tidyMeeting,
  useMeetingStore,
  whenTranslated,
  type MeetingPhase,
} from '../../stores/meetingStore'
import { MeetingMinutes } from './MeetingMinutes'
import { MeetingLanguages, MOST_LANGUAGES } from './MeetingLanguages'

const KNOWN_ERRORS = [
  'file_not_audio',
  'file_unreadable',
  'file_too_long',
  'local_model_not_installed',
  'speaker_models_not_installed',
  'system_audio_unsupported',
  'busy',
  'stt_invalid_key',
  'stt_quota_exceeded',
  'stt_timeout',
  'stt_failed',
]
const SPEAKERS_KEY = 'meeting.speakers'
const SYSTEM_SOUND_KEY = 'meeting.systemSound'
const TRANSLATE_KEY = 'meeting.translate'
const TRANSLATE_TO_KEY = 'meeting.translateTo'
const LANGUAGES_KEY = 'meeting.languages'
const POLISH_KEY = 'meeting.polishAfter'
const WORD_TIMES_KEY = 'meeting.wordTimes'

function rememberedChoice(key: string, fallback: string): string {
  try {
    return window.localStorage.getItem(key) || fallback
  } catch {
    return fallback
  }
}

function rememberChoice(key: string, value: string) {
  try {
    window.localStorage.setItem(key, value)
  } catch {
    // The choice then lasts for this window only.
  }
}

type Phase = MeetingPhase

/** A switch as the user left it; `fallback` until it has been touched. */
function remembered(key = SPEAKERS_KEY, fallback = false): boolean {
  try {
    const kept = window.localStorage.getItem(key)
    return kept === null ? fallback : kept === '1'
  } catch {
    return fallback
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
/** Work that goes on step by step: a node for each step, ticked once it is over, with a
 * bar that fills as the step under way goes on, and the time all of it still takes once
 * that can be told. `within` is how far the step under way has got, or null if it has
 * nothing to count. */
function Going({
  label,
  steps,
  at,
  within,
  value,
  left,
}: {
  label: string
  steps: { label: string; icon: ReactNode }[]
  at: number
  within: number | null
  value: number
  left: number | null
}) {
  return (
    <div className="flex w-full items-center gap-3">
      <div
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={value}
        className="flex flex-1 items-center gap-1.5"
      >
        {steps.map((step, index) => {
          const state = index < at ? 'done' : index === at ? 'now' : 'next'
          const filled = state === 'done' ? 100 : state === 'now' ? (within ?? 1) * 100 : 0
          return (
            <Fragment key={step.label}>
              <span
                aria-label={step.label}
                title={step.label}
                data-state={state}
                className={`flex h-5 w-5 shrink-0 items-center justify-center rounded-full transition-colors ${
                  state === 'done'
                    ? 'bg-accent text-white'
                    : state === 'now'
                      ? 'text-accent ring-1 ring-accent'
                      : 'text-text-tertiary ring-1 ring-border'
                }`}
              >
                {state === 'done' ? <Check size={12} strokeWidth={3} /> : step.icon}
              </span>
              <span className="relative h-1.5 flex-1 overflow-hidden rounded-full bg-bg-tertiary">
                <span
                  className="absolute inset-y-0 left-0 overflow-hidden rounded-full bg-accent transition-[width] duration-500"
                  style={{ width: `${filled}%` }}
                >
                  {state === 'now' && <span className="going-sweep" />}
                </span>
              </span>
            </Fragment>
          )
        })}
      </div>
      {left !== null && (
        <span className="flex items-center gap-1 text-[12px] tabular-nums text-text-tertiary">
          <Hourglass size={12} />
          {clock(left)}
        </span>
      )}
    </div>
  )
}

function when(time: number): string {
  return new Date(time).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
}

export function MeetingCard() {
  const { t } = useTranslation()
  const inputDevice = useAppStore((s) => s.config.input_device)
  const dictationModel = useAppStore((s) => s.config.stt_local_model)
  /** Whether the recognizer chosen in the settings is away from this machine: meetings
   * are heard with the same one, so their sound then goes there. */
  const heardAway = useAppStore(
    (s) => s.config.stt_provider !== 'local-qwen3' && s.config.stt_provider !== 'apple-speech',
  )
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
    editing,
    translating,
    translatingAt,
    sorting,
    tidying,
    unpolished,
    terms,
    making,
    progress,
    soFar,
    workingSince,
    times,
  } = useMeetingStore()
  const [past, setPast] = useState<MeetingSummary[]>([])
  const [query, setQuery] = useState('')
  /** The folder whose meetings the list shows; empty for all of them. */
  const [picked, setPicked] = useState(() => {
    try {
      return window.localStorage.getItem('meeting.folder') ?? ''
    } catch {
      return ''
    }
  })
  /** The meeting whose folder is being chosen. */
  const [moving, setMoving] = useState<string | null>(null)
  const folders = [...new Set(past.map((each) => each.folder?.trim() ?? ''))]
    .filter((name) => name !== '')
    .sort((a, b) => a.localeCompare(b))
  const inFolder = folders.includes(picked) ? picked : ''
  const pickFolder = (name: string) => {
    setPicked(name)
    try {
      window.localStorage.setItem('meeting.folder', name)
    } catch {
      // The choice then lasts only for this run.
    }
  }
  const [asking, setAsking] = useState(false)
  const [answer, setAnswer] = useState<string | null>(null)
  const askAll = () => {
    const question = query.trim()
    if (!question || asking) return
    setAsking(true)
    setAnswer(null)
    set({ error: null })
    Promise.resolve(meetingsAsk(question))
      .then((said) => setAnswer(said ?? ''))
      .catch((reason) =>
        set({
          error: String(reason).includes('llm_not_configured')
            ? 'meeting.error.llm_not_configured'
            : 'meeting.error.ask',
        }),
      )
      .finally(() => setAsking(false))
  }
  /** The meetings that hold what is looked for; null while nothing is looked for. */
  const [found, setFound] = useState<MeetingSummary[] | null>(null)
  useEffect(() => {
    const words = query.trim()
    if (!words) {
      setFound(null)
      return
    }
    let wanted = true
    const waited = setTimeout(() => {
      void Promise.resolve(meetingsSearch(words))
        .then((all) => {
          if (!wanted) return
          setFound(
            (all ?? []).map((each) => ({
              id: each.id,
              started_at: each.started_at,
              title: each.title,
              duration_s: each.duration_s,
              preview: each.found,
              has_minutes: false,
            })),
          )
        })
        .catch(() => {})
    }, 200)
    return () => {
      wanted = false
      clearTimeout(waited)
    }
  }, [query])
  const [showOptions, setShowOptions] = useState(false)
  const [showMore, setShowMore] = useState(false)
  const [counting, setCounting] = useState(false)
  // The templates laid out to pick from: for the kept meeting minutes are to be made of,
  // or with no meeting, only to look at, add to or take from.
  const [picking, setPicking] = useState<{ meeting: MeetingSummary | null } | null>(null)
  const [templates, setTemplates] = useState<MinutesTemplate[]>([])
  const [template, setTemplate] = useState(BUILT_IN)
  /** The kept sound of the meeting minutes are to be made of, and whether it is written
   * out anew first. */
  const [sound, setSound] = useState<string | null>(null)
  const [againFirst, setAgainFirst] = useState(true)
  const voicesAtHand = useMeetingStore((state) => state.voicesAtHand)
  const player = useRef<AudioHandle>(null)
  const label = t('meeting.speakerLabel')
  const refreshPast = () =>
    void Promise.resolve(meetingsList())
      .then((all) => setPast(all ?? []))
      .catch(() => {})
  const persist = () => void persistMeeting(label).then(refreshPast)
  /** Keeps a meeting that has just been written out, then names it after what was said. */
  /** Resolves once the meeting kept last has its name as well. */
  const named = useRef<Promise<void>>(Promise.resolve())
  const keep = () => {
    const kept = persistMeeting(label)
    named.current = kept.then(() => nameMeeting(label)).then(refreshPast)
    return kept
  }
  /** The files to be turned into text after the one in hand. */
  const [waiting, setWaiting] = useState<string[]>([])
  const waitingRef = useRef<string[]>([])
  const queue = (paths: string[]) => {
    waitingRef.current = paths
    setWaiting(paths)
  }
  /** Set while several files are worked through, and once one of them has failed. */
  const several = useRef({ running: false, failed: false })
  const set = useMeetingStore.setState
  const setText = (text: string | null) => set({ text })
  const setError = (error: string | null) => set({ error })
  const setKept = (kept: string | null) => set({ kept })
  const going = tidying !== null
  /** Counts the seconds while a file is worked on, so that the time shown moves. */
  const [, tick] = useState(0)
  const [level, setLevel] = useState({ seconds: 0, volume: 0, waiting: 0 })
  const liveRef = useRef<HTMLDivElement>(null)
  const [speakers, setSpeakers] = useState(false)
  const [speakersInstalled, setSpeakersInstalled] = useState(false)
  const [systemSound, setSystemSound] = useState(() => remembered(SYSTEM_SOUND_KEY))
  const [translate, setTranslate] = useState(() => remembered(TRANSLATE_KEY))
  const [target, setTarget] = useState(() => rememberedChoice(TRANSLATE_TO_KEY, 'en'))
  const [languages, setLanguages] = useState(() =>
    rememberedChoice(LANGUAGES_KEY, '').split(',').filter(Boolean).slice(0, MOST_LANGUAGES),
  )
  const [polishAfter, setPolishAfter] = useState(() => remembered(POLISH_KEY))
  const [position, setPosition] = useState(0)
  /** The text of the file being transcribed, as far as it is written out. */
  const [paused, setPaused] = useState(false)
  const notesRef = useRef<HTMLTextAreaElement>(null)
  const [download, setDownload] = useState<number | null>(null)
  /** The model that tells when each word was said: whether it is here, and wanted. */
  const [timesInstalled, setTimesInstalled] = useState(false)
  const [wordTimes, setWordTimes] = useState(false)
  const [timesDownload, setTimesDownload] = useState<number | null>(null)
  const exactTimes = useRef(false)
  exactTimes.current = wordTimes && timesInstalled
  /** After a recording: the time of each word, where the model for that is installed. */
  const hearAgain = () => (exactTimes.current ? timeWords(label) : Promise.resolve())
  const [voices, setVoices] = useState<string[]>([])
  const [tab, setTab] = useState('text')
  const [models, setModels] = useState<string[]>([])
  const [model, setModel] = useState<string | null>(null)
  const optionsRef = useRef<TranscribeOptions>({
    speakerLabel: null,
    modelId: null,
    languages: [],
  })

  optionsRef.current = {
    speakerLabel: speakers && speakersInstalled ? t('meeting.speakerLabel') : null,
    modelId: model,
    languages,
  }

  /** The text with each speaker who said next to nothing and sounds like another given
   * to that other. A voice that sounds like nobody else stays a person. */
  const settle = (text: string): Promise<string> => {
    const said = saidBy(text, label)
    if (Object.keys(said).length < 2) return Promise.resolve(text)
    return Promise.resolve(speakerFewer(said, 0))
      .then((pairs) => (pairs?.length ? withSpeakersJoined(text, label, pairs) : text))
      .catch(() => text)
  }
  /** Set once the file in hand was asked to be let be, until it has ended. */
  const [stopping, setStopping] = useState(false)
  const stopFile = () => {
    setStopping(true)
    void Promise.resolve(transcribeFileStop()).catch(() => {})
  }
  const enter = (next: Phase) => set({ phase: next })
  const now = () => useMeetingStore.getState().phase

  /** Shows the work of writing a file out and keeps what comes of it. With `same`, the
   * meeting in hand is written out anew from its own sound: it stays the same meeting,
   * with its sound, name, notes and minutes, and only its words and speakers are new. */
  const finish = (work: Promise<string>, same = false) => {
    const was = useMeetingStore.getState()
    setStopping(false)
    enter('working')
    set({
      progress: null,
      soFar: '',
      workingSince: Date.now(),
      text: null,
      recorded: false,
      error: null,
      names: {},
      voicesAtHand: true,
      times: [],
      unpolished: null,
      terms: null,
      ...(same
        ? {}
        : {
            minutes: null,
            id: null,
            notes: '',
            title: '',
            folder: inFolder,
            duration: 0,
            kept: null,
          }),
    })
    work
      .then((result) => settle(result ?? ''))
      .then((result) => {
        setText(result)
        if (same) {
          persist()
          return
        }
        keep()
        // The next file begins a meeting of its own, so this one is named first.
        return waitingRef.current.length > 0 ? named.current : undefined
      })
      .catch((reason) => {
        if (String(reason).includes('stopped')) {
          // What the user let be is no failure: the files that waited are let be as
          // well, and what was written of this one stands as a meeting.
          queue([])
          if (same) {
            set({ text: was.text, names: was.names, times: was.times, voicesAtHand: false })
            return
          }
          const soFar = useMeetingStore.getState().soFar
          if (!soFar.trim()) return
          return settle(soFar).then((text) => {
            setText(text)
            keep()
          })
        }
        const code = KNOWN_ERRORS.find((known) => String(reason).includes(known))
        setError(`meeting.error.${code ?? 'failed'}`)
        several.current.failed = several.current.running
        // A meeting that could not be written out anew stays as it was.
        if (same) {
          set({ text: was.text, names: was.names, times: was.times, voicesAtHand: false })
          return
        }
        // What was written of a new one before it failed is kept as a meeting.
        const soFar = useMeetingStore.getState().soFar
        if (!soFar.trim()) return
        return settle(soFar).then((text) => {
          setText(text)
          keep()
        })
      })
      .finally(() => {
        enter('idle')
        const [next, ...rest] = waitingRef.current
        if (next) {
          queue(rest)
          transcribeRef.current(next)
        } else {
          if (several.current.failed) setError('meeting.error.someFailed')
          several.current = { running: false, failed: false }
        }
      })
  }

  const transcribe = (path: string) => {
    if (now() !== 'idle' || useMeetingStore.getState().making) return
    finish(Promise.resolve(transcribeFile(path, optionsRef.current)))
  }
  const transcribeRef = useRef(transcribe)
  transcribeRef.current = transcribe
  /** Turns files into text one after another; files given while that goes on wait their turn. */
  const transcribeAll = (paths: string[]) => {
    if (paths.length === 0) return
    if (now() === 'working') {
      several.current.running = true
      queue([...waitingRef.current, ...paths])
    } else if (now() === 'idle') {
      several.current = { running: paths.length > 1, failed: false }
      queue(paths.slice(1))
      transcribe(paths[0])
    }
  }
  const transcribeAllRef = useRef(transcribeAll)
  transcribeAllRef.current = transcribeAll

  useEffect(() => {
    if (phase !== 'working' && !going) return
    const ticking = window.setInterval(() => tick((seconds) => seconds + 1), 1000)
    return () => window.clearInterval(ticking)
  }, [phase, going])

  useEffect(() => {
    const dropped = Promise.resolve(onFileDropped((paths) => transcribeAllRef.current(paths)))
    listenToMeeting()
    void recoverMeeting(label).then((found) => {
      if (found) void keep().then(hearAgain)
    })
    const downloading = Promise.resolve(
      onSpeakerModelsProgress(({ downloaded, total }) =>
        setDownload(total > 0 ? Math.round((downloaded / total) * 100) : 0),
      ),
    )
    void Promise.resolve(speakerModelsStatus())
      .then((status) => {
        if (!status) return
        setSpeakersInstalled(status.installed)
        // Telling speakers apart is on until the user turns it off.
        setSpeakers(status.installed && remembered(SPEAKERS_KEY, true))
      })
      .catch(() => {})
    const timing = Promise.resolve(
      onWordTimesProgress(({ downloaded, total }) =>
        setTimesDownload(total > 0 ? Math.round((downloaded / total) * 100) : 0),
      ),
    )
    void Promise.resolve(wordTimesStatus())
      .then((status) => {
        if (!status) return
        setTimesInstalled(status.installed)
        setWordTimes(status.installed && remembered(WORD_TIMES_KEY, true))
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
      void timing.then((stop) => stop?.())
      void dropped.then((stop) => stop?.())
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
    // The newest words stay in view, except while the user is changing earlier ones.
    const live = liveRef.current
    if (live && editing === null) live.scrollTop = live.scrollHeight
  }, [paragraphs, interim, editing])

  const toggleWordTimes = (on: boolean) => {
    if (!on || timesInstalled) {
      setWordTimes(on)
      remember(on, WORD_TIMES_KEY)
      return
    }
    if (timesDownload !== null) return
    setTimesDownload(0)
    setError(null)
    Promise.resolve(installWordTimes())
      .then(() => {
        setTimesInstalled(true)
        setWordTimes(true)
        remember(true, WORD_TIMES_KEY)
      })
      .catch(() => setError('meeting.error.timesDownload'))
      .finally(() => setTimesDownload(null))
  }

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

  const changeLanguages = (next: string[]) => {
    setLanguages(next)
    rememberChoice(LANGUAGES_KEY, next.join(','))
    // A meeting being recorded goes on in the languages now named.
    if (now() === 'recording') void Promise.resolve(meetingLanguages(next)).catch(() => {})
  }

  const bySpeaker = speakers && speakersInstalled

  const record = () => {
    if (now() !== 'idle' || useMeetingStore.getState().making) return
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
      folder: inFolder,
      duration: 0,
      interim: '',
      editing: null,
      translateTo: translate ? target : null,
      sides: {},
      me: null,
      times: [],
    })
    setPaused(false)
    setLevel({ seconds: 0, volume: 0, waiting: 0 })
    enter('recording')
    Promise.resolve(
      meetingRecordStart(inputDevice, bySpeaker, systemSound, model, languages),
    ).catch((reason) => {
      const code = KNOWN_ERRORS.find((known) => String(reason).includes(known))
      setError(`meeting.error.${code ?? 'microphone'}`)
      enter('idle')
    })
  }

  const pause = () => {
    if (now() !== 'recording') return
    const next = !paused
    setPaused(next)
    void Promise.resolve(meetingRecordPause(next)).catch(() => setPaused(!next))
  }

  const heardNow = () => useMeetingStore.getState().paragraphs
  const stop = (discard: boolean) => {
    if (now() !== 'recording') return
    enter('finishing')
    set({ interim: '', duration: level.seconds })
    Promise.resolve(meetingRecordStop(discard))
      .then(() => (discard ? undefined : whenTranslated()))
      // The sound becomes the meeting's own once the meeting is kept; that sets `kept`.
      .then(() => (discard ? undefined : settle(meetingText(heardNow(), label))))
      .then((said) => {
        if (said === undefined) return
        // From here on the meeting is its text, which the user can go on correcting.
        set({ recorded: true, text: said, editing: null, times: timesOf(heardNow()) })
        // Heard whole, the meeting tells its speakers apart better than window by window.
        void keep().then(hearAgain)
        if (polishAfter) polish(said)
      })
      .catch(() => setError('meeting.error.failed'))
      .finally(() => enter('idle'))
  }

  const again = () => {
    if (!kept || now() !== 'idle' || useMeetingStore.getState().making) return
    finish(Promise.resolve(transcribeFile(kept, optionsRef.current)), true)
  }

  const forget = () =>
    void Promise.resolve(id ? meetingAudioDelete(id) : undefined)
      .catch(() => {})
      .finally(() => setKept(null))

  const choose = () =>
    void Promise.resolve(pickMediaFile())
      .then((paths) => transcribeAll(paths ?? []))
      .catch(() => {})

  const percent =
    progress && progress.stage === 'text' && progress.total > 0
      ? Math.round((progress.done / progress.total) * 100)
      : null
  const left =
    percent !== null && workingSince !== null ? remaining(workingSince, percent / 100) : null
  const stepSpeakers = { label: t('meeting.stepSpeakers'), icon: <Users size={11} /> }
  const tidyingBar = tidying && (
    <Going
      label={t('meeting.polish')}
      steps={[
        { label: t('meeting.stepWords'), icon: <SpellCheck size={11} /> },
        ...(tidying.steps > 1 ? [stepSpeakers] : []),
      ]}
      at={Math.min(tidying.steps - 1, Math.floor(tidying.fraction * tidying.steps))}
      within={
        tidying.fraction * tidying.steps -
        Math.min(tidying.steps - 1, Math.floor(tidying.fraction * tidying.steps))
      }
      value={Math.round(tidying.fraction * 100)}
      left={remaining(tidying.since, tidying.fraction)}
    />
  )
  // A file is read, its speakers are told apart where that is asked for, and it is written.
  const told = optionsRef.current.speakerLabel !== null
  const workingBar = (
    <Going
      label={t('meeting.title')}
      steps={[
        { label: t('meeting.stepRead'), icon: <FileAudio size={11} /> },
        ...(told ? [stepSpeakers] : []),
        { label: t('meeting.stepText'), icon: <Type size={11} /> },
      ]}
      at={progress === null ? 0 : progress.stage === 'speakers' ? 1 : told ? 2 : 1}
      within={percent === null ? null : percent / 100}
      value={percent ?? 0}
      left={left}
    />
  )
  const pickTemplate = (meeting: MeetingSummary | null) => {
    setSound(null)
    // A meeting that has minutes was gone through before; its text stays unless asked.
    setAgainFirst(!meeting?.has_minutes)
    setPicking({ meeting })
    void Promise.resolve(minutesTemplates())
      .then((all) => setTemplates(all ?? []))
      .catch(() => {})
    if (meeting)
      void Promise.resolve(meetingAudio(meeting.id))
        .then((kept) => setSound(kept ?? null))
        .catch(() => {})
  }
  /** Makes minutes of the picked meeting in one go, after the picked template. */
  const makeOf = () => {
    const meeting = picking?.meeting
    setPicking(null)
    if (!meeting) return
    const again = sound && againFirst ? { audio: sound, options: optionsRef.current } : null
    void makeMinutes(label, meeting.id, templateText(t, templates, template), again).then(
      refreshPast,
    )
  }
  const makingBar = making && (
    <Going
      label={t('meeting.minutes')}
      steps={making.steps.map((step) => ({
        label: t(
          `meeting.step${step === 'text' ? 'Text' : step === 'words' ? 'Words' : step === 'speakers' ? 'Speakers' : 'Minutes'}`,
        ),
        icon:
          step === 'text' ? (
            <Type size={11} />
          ) : step === 'words' ? (
            <SpellCheck size={11} />
          ) : step === 'speakers' ? (
            <Users size={11} />
          ) : (
            <ScrollText size={11} />
          ),
      }))}
      at={making.at}
      within={making.within}
      value={Math.round(((making.at + making.within) / making.steps.length) * 100)}
      left={remaining(making.since, (making.at + making.within) / making.steps.length)}
    />
  )
  const open = (summary: MeetingSummary) =>
    void Promise.resolve(meetingGet(summary.id))
      .then((meeting) => {
        if (!meeting || now() !== 'idle') return
        setPosition(0)
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
          folder: meeting.folder ?? '',
          duration: meeting.duration_s ?? 0,
          times: meeting.times ?? [],
          terms: meeting.terms?.length
            ? { id: meeting.id, list: meeting.terms, open: false }
            : null,
          unpolished: meeting.unpolished ? { id: meeting.id, ...meeting.unpolished } : null,
        })
        void Promise.resolve(meetingAudio(meeting.id))
          .then((kept) => useMeetingStore.getState().id === meeting.id && setKept(kept ?? null))
          .catch(() => {})
      })
      .catch(() => {})

  /** Puts a meeting in a folder, or in none where `name` is empty. */
  const moveTo = (summary: MeetingSummary, name: string) => {
    setMoving(null)
    const folder = name.trim()
    void Promise.resolve(meetingFolder(summary.id, folder))
      .then(() => {
        if (useMeetingStore.getState().id === summary.id) set({ folder })
        refreshPast()
      })
      .catch(() => {})
  }

  const removePast = (summary: MeetingSummary) =>
    void Promise.resolve(meetingDelete(summary.id))
      .then(() => {
        if (useMeetingStore.getState().id === summary.id) set({ id: null })
        refreshPast()
      })
      .catch(() => {})

  const written = text ?? (recorded ? meetingText(paragraphs, label) : null)
  const speakerNumbers = written ? speakersIn(written, label) : []
  const result = written === null ? null : renameSpeakers(written, label, names)
  // Copied or saved, the text names a speaker once while they go on, as it reads here.
  const shared =
    written === null ? '' : renameSpeakers(withRunsJoined(written, label, names), label, names)
  // A text that carries times can also be saved as subtitles.
  const timedText = written !== null && subtitles(written, label, names, times, 'srt') !== ''
  const asSubtitles = timedText
    ? {
        srt: () => subtitles(plainOf(written), label, names, times, 'srt'),
        vtt: () => subtitles(plainOf(written), label, names, times, 'vtt'),
      }
    : undefined
  const notesBox = (
    <textarea
      ref={notesRef}
      value={notes}
      onChange={(event) => {
        if (now() !== 'recording') {
          set({ notes: event.target.value })
          return
        }
        // A note taken while the meeting runs carries the moment it was begun at.
        const typed = stampNote(event.target.value, event.target.selectionStart, level.seconds)
        set({ notes: typed.value })
        if (typed.value !== event.target.value) {
          window.requestAnimationFrame(() =>
            notesRef.current?.setSelectionRange(typed.caret, typed.caret),
          )
        }
      }}
      onBlur={persist}
      rows={3}
      aria-label={t('meeting.notes')}
      placeholder={t('meeting.notesPlaceholder')}
      className="mt-3 w-full resize-y rounded-[8px] border border-border bg-bg-primary px-3 py-2.5 text-[13px] leading-relaxed text-text-primary outline-none placeholder:text-text-tertiary"
    />
  )
  const polish = (source: string | null = written) =>
    void tidyMeeting(label, source ?? undefined).then(refreshPast)
  const heard = unpolished?.id === id ? unpolished : undefined
  const before = heard ? (heard.shaped ?? heard.text) : undefined
  /** Whether what the tidy-up changed is shown in the text, instead of the text as it
   * now reads. */
  const [comparing, setComparing] = useState(false)
  const unpolish = () => {
    if (!heard) return
    set({ text: heard.text, ...(heard.times ? { times: heard.times } : {}), unpolished: null })
    persist()
  }
  /** Keeps a speaker's name with the meeting, and with the voice where that is at hand. */
  const nameKept = (number: number, name: string) =>
    useMeetingStore.getState().voicesAtHand &&
    void Promise.resolve(rememberSpeaker(number, name))
      .then((all) => all && setVoices(all))
      .catch(() => {})
      .finally(persist)
  /** Writes each term the one way the user settled on, wherever the meeting says it,
   * and keeps the spelling in the dictionary. A term left empty stays as it is. */
  const settleTerms = (spellings: string[]) => {
    if (written === null || !terms) return
    const settled = terms.list
      .map((term, index) => ({
        spelling: spellings[index]?.trim() ?? '',
        others: [...term.others, term.spelling],
      }))
      .filter((term) => term.spelling !== '')
    // Each term is kept the way it is now written, so that it can be asked about again.
    const list = terms.list.map((term, index) => {
      const spelling = spellings[index]?.trim() || term.spelling
      const others = [...new Set([...term.others, term.spelling])].filter(
        (other) => other !== spelling,
      )
      return { spelling, others }
    })
    set({
      text: withSpellings(written, label, settled),
      terms: { ...terms, list, open: false },
    })
    for (const term of settled)
      void Promise.resolve(addDictionaryEntry(term.spelling, null)).catch(() => {})
    persist()
  }
  /** Takes the surplus speakers for the most alike of the `most` who said the most. */
  const fewer = (most: number) => {
    setCounting(false)
    if (written === null || !(most >= 1) || most >= speakerNumbers.length) return
    void Promise.resolve(speakerFewer(saidBy(written, label), most))
      .then((pairs) => {
        if (!pairs?.length) return
        set({ text: withSpeakersJoined(written, label, pairs) })
        persist()
      })
      .catch(() => {})
  }
  const giveTo = (line: number, speaker: number, all: boolean) => {
    if (written === null) return
    set({ text: withSpeakerAt(written, label, line, speaker, all) })
    persist()
  }
  const correct = (line: number, before: string, after: string) => {
    if (written === null) return
    set({ text: withLines(written, [line], [after]) })
    learnFrom(before, after)
    persist()
  }
  const chosenModel = model ?? (models.includes(dictationModel) ? dictationModel : models[0])

  const close = () => {
    setPosition(0)
    set({
      text: null,
      recorded: false,
      paragraphs: [],
      id: null,
      minutes: null,
      notes: '',
      names: {},
      title: '',
      folder: inFolder,
      duration: 0,
      kept: null,
      times: [],
    })
  }
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
                {speakerNumbers.length > 0 &&
                  (voicesAtHand && speakerNumbers.length > 1 ? (
                    counting ? (
                      <input
                        type="number"
                        autoFocus
                        min={1}
                        max={speakerNumbers.length}
                        defaultValue={speakerNumbers.length}
                        aria-label={t('meeting.people', { count: speakerNumbers.length })}
                        onBlur={(event) => fewer(Number(event.target.value))}
                        onKeyDown={(event) => {
                          if (event.key === 'Enter') fewer(Number(event.currentTarget.value))
                          if (event.key === 'Escape') setCounting(false)
                        }}
                        className="h-[18px] w-10 rounded-[4px] border border-border bg-bg-primary px-1 text-[12px] tabular-nums outline-none"
                      />
                    ) : (
                      <button
                        type="button"
                        onClick={() => setCounting(true)}
                        className="cursor-pointer border-none bg-transparent p-0 text-[12px] tabular-nums text-text-secondary underline decoration-dotted underline-offset-2 hover:text-text-primary"
                      >
                        {t('meeting.people', { count: speakerNumbers.length })}
                      </button>
                    )
                  ) : (
                    <span>{t('meeting.people', { count: speakerNumbers.length })}</span>
                  ))}
              </p>
            </>
          ) : (
            <h2 className="page-title">{t('meeting.title')}</h2>
          )}
        </div>
        {list && (
          <div className="flex shrink-0 items-center gap-2">
            {heardAway && (
              <span
                role="img"
                aria-label={t('meeting.heardAway')}
                title={t('meeting.heardAway')}
                className="inline-flex px-1 text-text-secondary"
              >
                <Cloud size={16} />
              </span>
            )}
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
              onClick={() => pickTemplate(null)}
              aria-label={t('meeting.template')}
              title={t('meeting.template')}
              className="btn-quiet px-2.5"
            >
              <LayoutTemplate size={16} />
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
                className={`h-2.5 w-2.5 rounded-full transition-transform ${
                  paused ? 'bg-text-tertiary' : 'bg-error'
                }`}
                style={{
                  transform: `scale(${paused ? 1 : 1 + Math.min(level.volume, 1) * 1.2})`,
                }}
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
                <button
                  type="button"
                  onClick={pause}
                  aria-pressed={paused}
                  aria-label={t(paused ? 'meeting.resume' : 'meeting.pauseRecording')}
                  title={t(paused ? 'meeting.resume' : 'meeting.pauseRecording')}
                  className="btn-quiet px-2.5"
                >
                  {paused ? <Play size={16} /> : <Pause size={16} />}
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
            {sorting && (
              <span
                role="status"
                aria-label={t('meeting.speakersAgain')}
                title={t('meeting.speakersAgain')}
                className="flex items-center px-1.5 text-text-tertiary"
              >
                <Users size={14} className="animate-pulse" />
              </span>
            )}
            {terms?.id === id && !terms.open && !going && written !== null && (
              <button
                type="button"
                onClick={() => set({ terms: { ...terms, open: true } })}
                aria-label={t('meeting.terms')}
                title={t('meeting.terms')}
                className="btn-quiet px-2.5"
              >
                <SpellCheck size={16} />
              </button>
            )}
            {before !== undefined && !going && (
              <button
                type="button"
                onClick={() => setComparing(!comparing)}
                aria-pressed={comparing}
                aria-label={t('meeting.compare')}
                title={t('meeting.compare')}
                className={`btn-quiet px-2.5 ${comparing ? 'text-accent' : ''}`}
              >
                <Diff size={16} />
              </button>
            )}
            <button
              type="button"
              onClick={() => polish()}
              disabled={going}
              aria-busy={going}
              aria-label={t('meeting.polish')}
              title={t('meeting.polish')}
              className="btn-quiet px-2.5 disabled:opacity-50"
            >
              <Sparkles size={16} className={going ? 'animate-pulse' : undefined} />
            </button>
            <button
              type="button"
              onClick={() => void copyText(plainOf(shared))}
              aria-label={t('meeting.copy')}
              title={t('meeting.copy')}
              className="btn-quiet px-2.5"
            >
              <Copy size={16} />
            </button>
            <button
              type="button"
              onClick={() =>
                void saveTextFile(shared, t('meeting.fileName'), asSubtitles, plainOf(shared))
              }
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
                {before !== undefined && (
                  <button type="button" role="menuitem" onClick={unpolish} className={item}>
                    {t('meeting.unpolish')}
                  </button>
                )}
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
                  rememberChoice(TRANSLATE_TO_KEY, event.target.value)
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
            <Toggle
              checked={polishAfter}
              onChange={(on) => {
                setPolishAfter(on)
                remember(on, POLISH_KEY)
              }}
              label={t('meeting.polishAfter')}
            />
            <Toggle
              checked={wordTimes && timesInstalled}
              onChange={toggleWordTimes}
              label={t('meeting.wordTimes')}
            />
            <MeetingLanguages chosen={languages} onChange={changeLanguages} />
            {timesDownload !== null && (
              <span className="text-[12px] text-text-secondary">
                {t('meeting.downloadingModel', { percent: timesDownload })}
              </span>
            )}
            {download !== null && (
              <span className="text-[12px] text-text-secondary">
                {t('meeting.downloadingSpeakers', { percent: download })}
              </span>
            )}
            {!heardAway && models.length > 1 && chosenModel && (
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
        <div className="flex flex-col gap-3 border-t border-border pt-4">
          {phase === 'recording' && (
            <MeetingLanguages chosen={languages} onChange={changeLanguages} />
          )}
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
                  <p className="text-[12px]">
                    {who !== null && (
                      <span className="font-medium" style={{ color: speakerColor(who) }}>
                        {`${names[who]?.trim() || (who > 0 ? `${label} ${who}` : label)}  `}
                      </span>
                    )}
                    <span className="tabular-nums text-text-tertiary">
                      {clock(Math.floor(paragraph.startMs / 1000))}
                    </span>
                  </p>
                  <p
                    contentEditable={phase === 'recording' ? 'plaintext-only' : undefined}
                    suppressContentEditableWarning
                    spellCheck={false}
                    data-paragraph={index}
                    onFocus={() => set({ editing: index })}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') {
                        event.preventDefault()
                        event.currentTarget.blur()
                      }
                    }}
                    onBlur={(event) => {
                      const typed = (event.currentTarget.textContent ?? '').trim()
                      if (typed) editParagraph(index, typed)
                      else event.currentTarget.textContent = paragraph.text
                      set({ editing: null })
                    }}
                    className="rounded-[3px] outline-none focus:bg-bg-secondary"
                  >
                    {paragraph.text}
                  </p>
                  {(paragraph.translation || (translatingAt === index && translating)) && (
                    <p className="text-text-secondary">
                      {[paragraph.translation, translatingAt === index ? translating : '']
                        .filter(Boolean)
                        .join(' ')}
                    </p>
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
                  ? t('meeting.working', { percent })
                  : t('meeting.reading')}
              <span role="timer" className="ml-3 tabular-nums text-text-tertiary">
                {clock(Math.max(0, Math.floor((Date.now() - (workingSince ?? Date.now())) / 1000)))}
              </span>
              <button
                type="button"
                onClick={stopFile}
                disabled={stopping}
                aria-label={t('meeting.stopFile')}
                title={t('meeting.stopFile')}
                className="ml-3 inline-flex h-6 w-6 items-center justify-center rounded-[6px] align-middle text-text-secondary hover:bg-bg-secondary hover:text-text-primary disabled:opacity-40"
              >
                <Square size={12} />
              </button>
              {waiting.length > 0 && (
                <span className="ml-3 inline-flex items-center gap-1 text-text-tertiary">
                  <Files size={13} />
                  <span aria-label={t('meeting.waiting')} title={t('meeting.waiting')}>
                    {waiting.length}
                  </span>
                </span>
              )}
            </p>
            <div className="mt-2">{workingBar}</div>
          </div>
          {soFar && (
            <div className="mt-4 opacity-80">
              <Transcript
                text={soFar}
                names={names}
                speakerLabel={label}
                label={t('meeting.live')}
              />
            </div>
          )}
        </div>
      )}

      {error && <p className="m-0 text-[12px] text-error">{t(error)}</p>}

      {list && past.length > 0 && (
        <label className="flex items-center gap-2 border-b border-border pb-2 text-text-tertiary">
          <Search size={14} className="flex-shrink-0" />
          <input
            type="search"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value)
              setAnswer(null)
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) askAll()
            }}
            aria-label={t('meeting.search')}
            placeholder={t('meeting.search')}
            className="h-7 min-w-0 flex-1 border-none bg-transparent text-[13px] text-text-primary outline-none placeholder:text-text-tertiary"
          />
          <button
            type="button"
            onClick={askAll}
            disabled={!query.trim() || asking}
            aria-label={t('meeting.askAll')}
            aria-busy={asking}
            title={t('meeting.askAll')}
            className="btn-quiet px-2 disabled:opacity-40"
          >
            <MessageCircleQuestion size={15} className={asking ? 'animate-pulse' : undefined} />
          </button>
        </label>
      )}
      {list && folders.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {['', ...folders].map((name) => (
            <button
              key={name}
              type="button"
              aria-pressed={inFolder === name}
              onClick={() => pickFolder(name)}
              className="flex h-7 cursor-pointer items-center gap-1.5 rounded-full border border-border bg-transparent px-3 text-[12px] text-text-secondary hover:text-text-primary aria-pressed:border-accent aria-pressed:text-accent"
            >
              {name !== '' && <Folder size={12} aria-hidden />}
              {name === '' ? t('meeting.allFolders') : name}
            </button>
          ))}
        </div>
      )}
      {list && answer !== null && (
        <p role="status" className="m-0 whitespace-pre-wrap text-[13px] leading-relaxed">
          {answer}
        </p>
      )}
      {picking && (
        <TemplateGallery
          templates={templates}
          onTemplates={setTemplates}
          chosen={template}
          onChoose={setTemplate}
          onClose={() => setPicking(null)}
          confirm={picking.meeting ? { label: t('meeting.minutes'), onConfirm: makeOf } : undefined}
        >
          {picking.meeting && sound && (
            <label className="flex cursor-pointer items-center gap-2 text-[12px] text-text-secondary">
              <input
                type="checkbox"
                checked={againFirst}
                onChange={(event) => setAgainFirst(event.target.checked)}
                className="accent-[var(--color-accent)]"
              />
              {t('meeting.againFirst')}
            </label>
          )}
        </TemplateGallery>
      )}
      {list &&
        (past.length > 0 ? (
          <ul
            className="m-0 list-none divide-y divide-border border-y border-border p-0"
            aria-label={t('meeting.pastList')}
          >
            {(
              found ?? past.filter((each) => inFolder === '' || (each.folder ?? '') === inFolder)
            ).map((summary) => (
              <li key={summary.id} className="group flex items-center gap-3 py-3">
                <button
                  type="button"
                  onClick={() => open(summary)}
                  className="flex min-w-0 flex-1 cursor-pointer flex-col gap-0.5 border-none bg-transparent p-0 text-left"
                >
                  <span className="truncate text-[13px] font-medium text-text-primary">
                    {summary.title?.trim() || plainOf(summary.preview)}
                  </span>
                  {found && summary.title?.trim() && (
                    <span className="truncate text-[12px] text-text-secondary">
                      {summary.preview}
                    </span>
                  )}
                  <span className="text-[12px] tabular-nums text-text-tertiary">
                    {when(summary.started_at)}
                    {(summary.duration_s ?? 0) > 0 && (
                      <span className="ml-4">{clock(summary.duration_s ?? 0)}</span>
                    )}
                    {inFolder === '' && summary.folder && (
                      <span className="ml-4 inline-flex items-center gap-1">
                        <Folder size={11} aria-hidden />
                        {summary.folder}
                      </span>
                    )}
                  </span>
                  {tidying?.id === summary.id && tidyingBar}
                  {making?.id === summary.id && makingBar}
                </button>
                {summary.has_minutes && (
                  <span className="text-[12px] text-text-tertiary">{t('meeting.tabMinutes')}</span>
                )}
                <button
                  type="button"
                  aria-label={t('meeting.minutes')}
                  title={t('meeting.minutes')}
                  disabled={making !== null || tidying !== null}
                  onClick={() => pickTemplate(summary)}
                  className="btn-quiet px-2.5 disabled:opacity-50"
                >
                  <ScrollText
                    size={15}
                    className={making?.id === summary.id ? 'animate-pulse' : undefined}
                  />
                </button>
                <div className="relative flex">
                  <button
                    type="button"
                    aria-label={t('meeting.folder')}
                    title={t('meeting.folder')}
                    aria-expanded={moving === summary.id}
                    onClick={() => setMoving(moving === summary.id ? null : summary.id)}
                    className="cursor-pointer border-none bg-transparent p-1 text-text-tertiary opacity-0 transition-opacity hover:text-text-primary focus-visible:opacity-100 group-hover:opacity-100 aria-expanded:opacity-100"
                  >
                    <FolderInput size={14} />
                  </button>
                  {moving === summary.id && (
                    <div
                      role="menu"
                      className="absolute right-0 top-full z-20 mt-1 flex min-w-[180px] flex-col rounded-[8px] border border-border bg-bg-elevated py-1 shadow-lg"
                    >
                      {folders
                        .filter((name) => name !== (summary.folder ?? ''))
                        .map((name) => (
                          <button
                            key={name}
                            type="button"
                            role="menuitem"
                            onClick={() => moveTo(summary, name)}
                            className={item}
                          >
                            {name}
                          </button>
                        ))}
                      {summary.folder && (
                        <button
                          type="button"
                          role="menuitem"
                          onClick={() => moveTo(summary, '')}
                          className={item}
                        >
                          {t('meeting.noFolder')}
                        </button>
                      )}
                      <input
                        aria-label={t('meeting.newFolder')}
                        placeholder={t('meeting.newFolder')}
                        maxLength={40}
                        onKeyDown={(event) => {
                          if (event.key === 'Enter' && event.currentTarget.value.trim())
                            moveTo(summary, event.currentTarget.value)
                          if (event.key === 'Escape') setMoving(null)
                        }}
                        className="mx-2 my-1 h-7 rounded-[6px] border border-border bg-bg-primary px-2 text-[12px] outline-none focus:border-accent"
                      />
                    </div>
                  )}
                </div>
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
            {kept && (
              <MeetingAudio
                key={kept}
                ref={player}
                src={audioSource(kept)}
                onPosition={setPosition}
              />
            )}
            {kept && stampedNotes(notes).length > 0 && (
              <ul
                className="m-0 flex list-none flex-col gap-1 border-l-2 border-border p-0 pl-3"
                aria-label={t('meeting.notes')}
              >
                {stampedNotes(notes).map((note, index) => (
                  <li key={index}>
                    <button
                      type="button"
                      onClick={() => player.current?.seek(note.seconds, true)}
                      className="flex cursor-pointer gap-3 border-none bg-transparent p-0 text-left text-[13px] text-text-primary hover:text-accent"
                    >
                      <span className="tabular-nums text-text-tertiary">{note.time}</span>
                      {note.text}
                    </button>
                  </li>
                ))}
              </ul>
            )}
            {tidying?.id === id && tidyingBar}
            {terms?.id === id && terms.open && !tidying && written !== null && (
              <MeetingTerms
                key={terms.list.map((term) => term.spelling).join('\n')}
                terms={terms.list}
                text={written}
                speakerLabel={label}
                times={times}
                onHear={kept ? (seconds) => player.current?.seek(seconds, true) : undefined}
                onSettle={settleTerms}
                onSkip={() => set({ terms: { ...terms, open: false } })}
              />
            )}
            {speakerNumbers.length > 0 && (
              <div className="flex flex-wrap gap-2">
                {speakerNumbers.map((number) => (
                  <input
                    key={number}
                    value={names[number] ?? ''}
                    onChange={(event) => set({ names: { ...names, [number]: event.target.value } })}
                    onBlur={(event) => nameKept(number, event.target.value)}
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
              onSeek={kept ? (seconds, play) => player.current?.seek(seconds, play) : undefined}
              position={kept ? position : undefined}
              times={times}
              onEdit={correct}
              before={comparing ? before : undefined}
              onSpeaker={giveTo}
              allLabel={t('meeting.allOfSpeaker')}
              onName={(number, name) => {
                set({ names: { ...names, [number]: name } })
                nameKept(number, name)
              }}
              nameLabel={t('meeting.speakerName')}
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
                  void nameFromMinutes(label, written).then(refreshPast)
                }}
                onSeek={kept ? (seconds) => player.current?.seek(seconds, true) : undefined}
                onEdit={(changed) => {
                  set({ minutes: changed })
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
