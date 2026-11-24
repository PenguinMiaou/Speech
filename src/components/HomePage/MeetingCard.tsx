import { Fragment, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import {
  AudioLines,
  ArrowDownUp,
  ArrowLeft,
  Cloud,
  Download,
  Files,
  Ellipsis,
  Folder,
  FolderInput,
  FolderMinus,
  FolderOpen,
  Pencil,
  Mic,
  Pause,
  Scissors,
  Play,
  SlidersHorizontal,
  Check,
  FileAudio,
  Hourglass,
  SpellCheck,
  Diff,
  Type,
  Square,
  Trash2,
  Users,
  Search,
  MessageCircleQuestion,
  MousePointerClick,
  X,
  LayoutTemplate,
  ScrollText,
  PenLine,
} from 'lucide-react'
import {
  addDictionaryEntry,
  minutesTemplates,
  type MinutesTemplate,
  forgetSpeaker,
  installSpeakerModels,
  installWordTimes,
  listLocalModels,
  audioSource,
  meetingAudio,
  meetingAudioDelete,
  meetingAudioTrim,
  meetingAudioCutUndo,
  meetingAudioCutKeep,
  meetingDelete,
  meetingFolder,
  meetingFolderPick,
  meetingGet,
  meetingSave,
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
import { NAV_AGAIN } from '../../lib/router'
import { openContextMenu } from '../ContextMenu'
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
  notesShown,
  withNotesTyped,
  stampedNotes,
  withLines,
  withRunsJoined,
  withSpeakerAt,
  withoutSection,
  transcriptLines,
  secondsOf,
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
  soundOf,
  timeWords,
  persistMeeting,
  tidyMeeting,
  useMeetingStore,
  whenTranslated,
  type MeetingPhase,
} from '../../stores/meetingStore'
import { MeetingMinutes } from './MeetingMinutes'
import { MeetingOverviewPanel } from './MeetingOverviewPanel'
import { MinutesForm } from './MinutesForm'
import { CopyButton } from '../CopyButton'
import { useDismiss } from '../../hooks/useDismiss'
import { create } from 'zustand'
import { forgetPacked } from '../../stores/meetingStore'
import { toast } from '../toast-service'
import { byDay, dayName, MEETING_ORDERS, ordered, type MeetingOrder } from '../../lib/meeting-list'
import { FindInText, type FindHandle } from './FindInText'
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
/** Below this level the microphone hears nothing at all, not even the room; after this many
 * seconds of a recording that is told. */
const MIC_DEAD_LEVEL = 0.0005
/** The sound kept after the last words when a silent end is cut off. */
const TAIL_KEPT_MS = 3_000
/** How much sound after the last words is offered to be cut off. */
const SILENT_TAIL_MS = 60_000
const MIC_DEAD_AFTER = 4
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

/** How many lines of the list of meetings are shown at a time. */
const PAGE = 60

function timeOf(time: number): string {
  return new Date(time).toLocaleTimeString(undefined, { timeStyle: 'short' })
}

function when(time: number): string {
  return new Date(time).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
}

/** Meetings and sounds deleted and waiting out the time to undo it, hidden meanwhile.
 * Kept outside the page, so that they stay hidden when it is left and come back to. */
export const useGoing = create<{ meetings: string[]; sounds: string[] }>(() => ({
  meetings: [],
  sounds: [],
}))
const markGoing = (key: 'meetings' | 'sounds', id: string, on: boolean) =>
  useGoing.setState((state) => ({
    [key]: on ? [...state[key], id] : state[key].filter((each) => each !== id),
  }))

/** A meeting's text, times and length, as they were before a cut, to put back. */
type BeforeCut = Pick<
  ReturnType<typeof useMeetingStore.getState>,
  'text' | 'times' | 'duration' | 'unpolished'
>

export function MeetingCard() {
  const { t, i18n } = useTranslation()
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
    answers,
    overview,
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
    namesFound,
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
  /** The order the list is read in, kept for the next time. */
  const [order, setOrder] = useState<MeetingOrder>(() => {
    try {
      const kept = window.localStorage.getItem('meeting.order') as MeetingOrder | null
      return kept && MEETING_ORDERS.includes(kept) ? kept : 'newest'
    } catch {
      return 'newest'
    }
  })
  const chooseOrder = (next: MeetingOrder) => {
    setOrder(next)
    setShownCount(PAGE)
    try {
      window.localStorage.setItem('meeting.order', next)
    } catch {
      // The choice then lasts only for this run.
    }
  }
  /** How many lines of the list are shown; more are shown on asking. */
  const [shownCount, setShownCount] = useState(PAGE)
  /** The folder being renamed, and the name typed for it. */
  const [renaming, setRenaming] = useState<string | null>(null)
  /** The meeting whose folder is being chosen. */
  const [moving, setMoving] = useState<string | null>(null)
  const movingRef = useRef<HTMLDivElement>(null)
  const closeMoving = useCallback(() => setMoving(null), [])
  useDismiss(moving !== null, closeMoving, movingRef)
  const folders = [...new Set(past.map((each) => each.folder?.trim() ?? ''))]
    .filter((name) => name !== '')
    .sort((a, b) => a.localeCompare(b))
  const inFolder = folders.includes(picked) ? picked : ''
  const pickFolder = (name: string) => {
    setPicked(name)
    setShownCount(PAGE)
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
  const goingMeetings = useGoing((state) => state.meetings)
  const finder = useRef<FindHandle>(null)
  const textBox = useRef<HTMLDivElement>(null)
  /** Whether the user has seen and put away the hint on changing words and speakers. */
  const [editHintSeen, setEditHintSeen] = useState(() => {
    try {
      return window.localStorage.getItem('meeting.editHintSeen') === '1'
    } catch {
      return false
    }
  })
  const seeEditHint = () => {
    setEditHintSeen(true)
    try {
      window.localStorage.setItem('meeting.editHintSeen', '1')
    } catch {
      // Shown again next time.
    }
  }
  const [showMore, setShowMore] = useState(false)
  const moreRef = useRef<HTMLDivElement>(null)
  const closeMore = useCallback(() => setShowMore(false), [])
  useDismiss(showMore, closeMore, moreRef)
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
    named.current = kept
      .then(() => nameMeeting(label))
      .then(refreshPast)
      .then(() => offerFolder(useMeetingStore.getState().id))
    return kept
  }
  /** The folder the meeting just kept may go in, waiting for the user to say yes or no. */
  const [folderOffer, setFolderOffer] = useState<{
    id: string
    title: string
    folder: string
  } | null>(null)
  /** Asks the language model which of the user's folders a meeting just kept belongs in,
   * where it is in none, and offers that folder. */
  const offerFolder = (id: string | null) => {
    if (!id) return Promise.resolve()
    return Promise.resolve(meetingsList())
      .then((all) => {
        const list = all ?? []
        const meeting = list.find((each) => each.id === id)
        if (!meeting || meeting.folder?.trim()) return
        const held = new Map<string, string[]>()
        for (const each of list) {
          const folder = each.folder?.trim()
          if (!folder || each.id === id) continue
          held.set(folder, [
            ...(held.get(folder) ?? []),
            each.title?.trim() || plainOf(each.preview),
          ])
        }
        if (held.size === 0) return
        const state = useMeetingStore.getState()
        const title = meeting.title?.trim() || (state.id === id ? state.title.trim() : '')
        const transcript = state.id === id ? (state.text ?? '') : meeting.preview
        return Promise.resolve(
          meetingFolderPick(
            title,
            transcript,
            [...held].map(([name, titles]) => ({ name, titles })),
          ),
        ).then((folder) => {
          if (folder && held.has(folder))
            setFolderOffer({ id, title: title || plainOf(meeting.preview), folder })
        })
      })
      .catch(() => {})
  }
  const acceptFolder = (offer: { id: string; folder: string }) => {
    setFolderOffer(null)
    void Promise.resolve(meetingFolder(offer.id, offer.folder))
      .then(() => {
        if (useMeetingStore.getState().id === offer.id) set({ folder: offer.folder })
        refreshPast()
      })
      .catch(() => toast.error(t('meeting.error.folder')))
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
  /** The loudest the microphone has been in this recording, to tell one that hears nothing
   * at all: muted, or not allowed. */
  const [loudest, setLoudest] = useState(0)
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
  /** The minutes with nothing said after which the recording paused itself; null while it
   * did not. */
  const [quietPaused, setQuietPaused] = useState<number | null>(null)
  /** Bumped when the kept sound is cut in place, so the player reads it afresh. */
  const [soundVersion, setSoundVersion] = useState(0)
  const [cuttingTail, setCuttingTail] = useState(false)
  /** The second the user asked to cut the meeting off at, waiting to be confirmed. */
  const [cuttingAt, setCuttingAt] = useState<number | null>(null)
  /** Pause requests not answered yet: until they are, the paused state the recording
   * reports may be older than the user's click. */
  const pausing = useRef(0)
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
  const [tab, setTab] = useState<'text' | 'overview' | 'minutes'>('text')
  const [models, setModels] = useState<string[]>([])
  /** How many of the settings a recording needs, read when the page opens, are in. */
  const [prepared, setPrepared] = useState(0)
  const recordAsked = useMeetingStore((s) => s.recordAsked)
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
      namesFound: null,
      ...(same
        ? {}
        : {
            minutes: null,
            answers: [],
            overview: null,
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
      .finally(() => setPrepared((done) => done + 1))
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
      .finally(() => setPrepared((done) => done + 1))
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
        .then((now) => {
          if (!now) return
          setLevel(now)
          setLoudest((was) => Math.max(was, now.volume))
          // The recording pauses itself after a while with nothing said, also while this
          // page is not open, so whether it is paused comes from the recording.
          if (pausing.current === 0 && typeof now.paused === 'boolean') {
            setPaused(now.paused)
            setQuietPaused(now.quiet_paused ?? null)
          }
        })
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
    if (now() === 'recording')
      void Promise.resolve(meetingLanguages(next)).catch(() =>
        toast.error(t('meeting.error.languages')),
      )
  }

  const bySpeaker = speakers && speakersInstalled

  const record = () => {
    if (now() !== 'idle' || useMeetingStore.getState().making) return
    setQuietPaused(null)
    setTab((shown) => (shown === 'overview' ? 'text' : shown))
    set({
      error: null,
      text: null,
      recorded: false,
      kept: null,
      paragraphs: [],
      notes: '',
      names: {},
      minutes: null,
      answers: [],
      overview: null,
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
    setLoudest(0)
    enter('recording')
    Promise.resolve(
      meetingRecordStart(inputDevice, bySpeaker, systemSound, model, languages),
    ).catch((reason) => {
      const code = KNOWN_ERRORS.find((known) => String(reason).includes(known))
      setError(`meeting.error.${code ?? 'microphone'}`)
      enter('idle')
    })
  }

  // The user asked from the offer window to record a meeting going on: it begins once the
  // settings a recording needs are in.
  useEffect(() => {
    if (!recordAsked || prepared < 2) return
    useMeetingStore.setState({ recordAsked: false })
    if (now() === 'idle') record()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recordAsked, prepared])

  const pause = () => {
    if (now() !== 'recording') return
    const next = !paused
    setQuietPaused(null)
    setPaused(next)
    pausing.current += 1
    void Promise.resolve(meetingRecordPause(next))
      .catch(() => setPaused(!next))
      .finally(() => {
        pausing.current -= 1
      })
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

  /** Deletes the kept sound of the open meeting, after a time in which it can be undone. */
  const forget = () => {
    if (!id) return
    const forId = id
    const was = kept
    setKept(null)
    markGoing('sounds', forId, true)
    toast.undoable(t('meeting.audioDeleted'), {
      label: t('common.undo'),
      undo: () => {
        markGoing('sounds', forId, false)
        if (useMeetingStore.getState().id === forId) setKept(was)
      },
      commit: () =>
        void Promise.resolve(meetingAudioDelete(forId))
          .catch(() => toast.error(t('meeting.error.audioDelete')))
          .finally(() => markGoing('sounds', forId, false)),
    })
  }

  const choose = () =>
    void Promise.resolve(pickMediaFile())
      .then((paths) => transcribeAll(paths ?? []))
      .catch(() => toast.error(t('meeting.error.pick')))

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
        .then((kept) => setSound(soundOf(meeting.id, kept)))
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
        // The overview of another meeting is not asked for by itself on opening this one.
        setTab((shown) => (shown === 'overview' ? 'text' : shown))
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
          answers: meeting.answers ?? [],
          overview: meeting.overview ?? null,
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
          .then(
            (kept) =>
              useMeetingStore.getState().id === meeting.id &&
              !useGoing.getState().sounds.includes(meeting.id) &&
              setKept(soundOf(meeting.id, kept)),
          )
          .catch(() => {})
      })
      .catch(() => toast.error(t('meeting.error.open')))

  /** Puts a meeting in a folder, or in none where `name` is empty. */
  const moveTo = (summary: MeetingSummary, name: string) => {
    setMoving(null)
    const folder = name.trim()
    void Promise.resolve(meetingFolder(summary.id, folder))
      .then(() => {
        if (useMeetingStore.getState().id === summary.id) set({ folder })
        refreshPast()
      })
      .catch(() => toast.error(t('meeting.error.folder')))
  }

  /** Deletes a meeting, after a time in which it can be undone; hidden meanwhile. */
  const deleteMeeting = (meetingId: string, title: string) => {
    if (useMeetingStore.getState().id === meetingId) closeRef.current()
    markGoing('meetings', meetingId, true)
    toast.undoable(t('meeting.deleted', { title }), {
      label: t('common.undo'),
      undo: () => markGoing('meetings', meetingId, false),
      // Stays hidden once deleted, so that a list read before that does not show it again.
      commit: () =>
        void Promise.resolve(meetingDelete(meetingId))
          .then(refreshPast)
          .catch(() => {
            markGoing('meetings', meetingId, false)
            toast.error(t('meeting.error.delete'))
          }),
    })
  }
  /** Gives every meeting in folder `from` the folder `to`; none where `to` is empty. */
  const refile = (from: string, to: string) =>
    Promise.all(
      past
        .filter((each) => (each.folder ?? '') === from)
        .map((each) => Promise.resolve(meetingFolder(each.id, to))),
    )
      .then(() => {
        if (useMeetingStore.getState().folder === from) set({ folder: to })
        refreshPast()
      })
      .catch(() => toast.error(t('meeting.error.folder')))
  const renameFolder = (from: string, to: string) => {
    setRenaming(null)
    const name = to.trim()
    if (!name || name === from) return
    void refile(from, name).then(() => pickFolder(name))
  }
  /** Takes a folder away, its meetings kept and in no folder, with a way to undo it. */
  const removeFolder = (name: string) => {
    const held = past.filter((each) => (each.folder ?? '') === name).map((each) => each.id)
    pickFolder('')
    void refile(name, '')
    toast.undoable(t('meeting.folderRemoved', { folder: name }), {
      label: t('common.undo'),
      commit: () => {},
      undo: () =>
        void Promise.all(held.map((id) => Promise.resolve(meetingFolder(id, name))))
          .then(() => {
            refreshPast()
            pickFolder(name)
          })
          .catch(() => toast.error(t('meeting.error.folder'))),
    })
  }
  const removePast = (summary: MeetingSummary) =>
    deleteMeeting(summary.id, summary.title?.trim() || plainOf(summary.preview))

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
      value={notesShown(notes)}
      // A note taken while the meeting runs keeps the moment its line was begun at, out of
      // sight, so that it can be found in the recording.
      onChange={(event) =>
        set({
          notes: withNotesTyped(
            notes,
            event.target.value,
            now() === 'recording' ? level.seconds : null,
          ),
        })
      }
      onBlur={persist}
      rows={3}
      aria-label={t('meeting.notes')}
      placeholder={t('meeting.notesPlaceholder')}
      className="mt-3 w-full resize-y rounded-md border border-border bg-bg-primary px-3 py-2.5 text-[13px] leading-relaxed text-text-primary outline-none placeholder:text-text-tertiary"
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
      void Promise.resolve(addDictionaryEntry(term.spelling, null)).catch(() =>
        toast.error(t('meeting.error.dictionary')),
      )
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
  /** Takes a section out of the meeting's text, with its times. The text as it was before
   * the tidy-up loses the section too where it still lines up with the text, and is let go
   * otherwise, so that the words taken out are not kept there. */
  const cut = (line: number) => {
    if (written === null || !id) return
    const left = withoutSection(written, label, line)
    if (!left) return
    settleLastCut()
    const before = beforeCut()
    const keptTimes =
      times.length > left.heading ? times.filter((_, at) => at !== left.heading) : times
    const lines = written.split('\n').length
    const cutOf = (text: string | undefined) =>
      text !== undefined && text.split('\n').length === lines
        ? withoutSection(text, label, line)?.text
        : undefined
    const was = heard ? { text: cutOf(heard.text), shaped: cutOf(heard.shaped) } : null
    const stillHeard =
      heard && was?.text !== undefined && (heard.shaped === undefined || was.shaped !== undefined)
        ? {
            ...heard,
            text: was.text,
            ...(was.shaped === undefined ? {} : { shaped: was.shaped }),
            ...(heard.times ? { times: heard.times.filter((_, at) => at !== left.heading) } : {}),
          }
        : null
    set({ text: left.text, times: keptTimes, ...(heard ? { unpolished: stillHeard } : {}) })
    persist()
    offerUndo(id, before, false)
  }
  /** Cuts the kept sound off at `endMs` from its start, before anything else is cut, so
   * that the text is not cut where the sound could not be. Whether it was, or there is no
   * sound; shows why where it could not be. */
  /** The undo of the last cut, while it can still be asked for. */
  const lastCut = useRef<{
    forId: string
    sound: boolean
    now: () => void
    superseded: boolean
  } | null>(null)
  const beforeCut = (): BeforeCut => {
    const { text, times, duration, unpolished } = useMeetingStore.getState()
    return { text, times, duration, unpolished }
  }
  /** Says a cut was made, with a way to undo it for a while: the text, times and length
   * as they were, and the sound where it was cut too. */
  const offerUndo = (forId: string, before: BeforeCut, sound: boolean) => {
    const cut = { forId, sound, now: () => {}, superseded: false }
    cut.now = toast.undoable(t('meeting.cutDone'), {
      label: t('common.undo'),
      // A cut after this one has already let this one's sound go.
      commit: () => {
        if (sound && !cut.superseded)
          void Promise.resolve(meetingAudioCutKeep(forId)).catch(() => {})
      },
      undo: () =>
        void (async () => {
          try {
            if (sound && !(await meetingAudioCutUndo(forId))) throw new Error('nothing kept')
            if (sound) forgetPacked(forId)
            if (useMeetingStore.getState().id === forId) {
              set(before)
              if (sound) {
                // Put back in the form it had before the cut, which may not be the one shown.
                const back = await meetingAudio(forId)
                if (useMeetingStore.getState().id === forId) setKept(soundOf(forId, back))
                setSoundVersion((version) => version + 1)
              }
              persist()
              return
            }
            const record = await meetingGet(forId)
            await meetingSave({
              ...record,
              transcript: before.text ?? record.transcript,
              times: before.times,
              duration_s: before.duration,
              unpolished: before.unpolished
                ? {
                    text: before.unpolished.text,
                    shaped: before.unpolished.shaped,
                    times: before.unpolished.times,
                  }
                : null,
            })
            refreshPast()
          } catch {
            toast.error(t('meeting.error.undo'))
          }
        })(),
    })
    lastCut.current = cut
  }
  /** A new cut lets the one before stand: only the last can be undone. A new cut of the
   * same meeting's sound lets the sound kept aside go itself; any other leaves that to the
   * one before. */
  const settleLastCut = (forId?: string, sound = false) => {
    const cut = lastCut.current
    lastCut.current = null
    if (!cut) return
    cut.superseded = sound && cut.sound && cut.forId === forId
    cut.now()
  }
  const trimSound = async (forId: string, endMs: number) => {
    settleLastCut(forId, Boolean(kept))
    if (!kept) return true
    try {
      await meetingAudioTrim(forId, endMs)
    } catch {
      if (useMeetingStore.getState().id === forId) setError('meeting.error.trim')
      return false
    }
    // The player reads the shortened sound afresh.
    if (useMeetingStore.getState().id === forId) setSoundVersion((version) => version + 1)
    return true
  }
  /** Cuts the meeting off from the section whose heading is at `line` to the end: the kept
   * sound from that section's time on, then its text and the times of that text. The text
   * as it was before the tidy-up is cut the same where it still lines up, and let go
   * otherwise. */
  const cutAfter = async (line: number, cutMs?: number) => {
    if (written === null || !id) return
    const parsed = transcriptLines(written, label)
    const at = parsed[line]
    if (!at || 'text' in at) return
    const forId = id
    const forText = written
    const endMs = cutMs ?? secondsOf(at.time) * 1000
    setError(null)
    const before = beforeCut()
    const sound = Boolean(kept)
    if (!(await trimSound(forId, endMs))) return
    const now = useMeetingStore.getState()
    if (now.id !== forId || now.text !== forText) return
    const headingsBefore = parsed.slice(0, line).filter((each) => !('text' in each)).length
    const all = forText.split('\n')
    const upTo = (text: string) => {
      const lines = text.split('\n').slice(0, line)
      while (lines.length > 0 && lines[lines.length - 1].trim() === '') lines.pop()
      return lines.join('\n')
    }
    const cutOf = (text: string | undefined) =>
      text !== undefined && text.split('\n').length === all.length ? upTo(text) : undefined
    const was = heard ? { text: cutOf(heard.text), shaped: cutOf(heard.shaped) } : null
    const stillHeard =
      heard && was?.text !== undefined && (heard.shaped === undefined || was.shaped !== undefined)
        ? {
            ...heard,
            text: was.text,
            ...(was.shaped === undefined ? {} : { shaped: was.shaped }),
            ...(heard.times ? { times: heard.times.slice(0, headingsBefore) } : {}),
          }
        : null
    set({
      text: upTo(forText),
      times: times.slice(0, headingsBefore),
      duration: Math.floor(endMs / 1000),
      ...(heard ? { unpolished: stillHeard } : {}),
    })
    persist()
    offerUndo(forId, before, sound)
  }
  /** Each section's line and the moment it begins, read once per text. */
  const sectionStarts = useMemo(
    () =>
      written === null
        ? []
        : transcriptLines(written, label).flatMap((line, index) =>
            'text' in line ? [] : [{ line: index, ms: secondsOf(line.time) * 1000 }],
          ),
    [written, label],
  )
  /** Where the meeting can be cut off at `ms`: the first section that begins there or
   * later, -1 where none does, and null where the cut would leave no text (at or before
   * the first section). */
  const cutLineAt = (ms: number): number | null => {
    if (sectionStarts.length === 0 || ms <= sectionStarts[0].ms) return null
    return sectionStarts.find((section) => section.ms >= ms)?.line ?? -1
  }
  /** Cuts the meeting off where the player stands: the sound from there on, and the
   * sections that begin there or later. */
  const cutAtPlayer = async (ms: number) => {
    const line = cutLineAt(ms)
    if (line === null || !id) return
    if (line >= 0) return cutAfter(line, ms)
    const forId = id
    setError(null)
    const before = beforeCut()
    const sound = Boolean(kept)
    if (!(await trimSound(forId, ms))) return
    if (useMeetingStore.getState().id !== forId) return
    set({ duration: Math.floor(ms / 1000) })
    persist()
    offerUndo(forId, before, sound)
  }
  /** Where the last words end, with a moment after them; null where that is not known. */
  const lastWordsMs = times.length > 0 ? times[times.length - 1].end_ms + TAIL_KEPT_MS : null
  /** Cuts the kept sound off a moment after the last words, for a recording left running
   * after the meeting ended with nothing more said. The text stays as it is. */
  const cutSilentTail = async () => {
    if (!id || lastWordsMs === null) return
    const forId = id
    setError(null)
    const before = beforeCut()
    const sound = Boolean(kept)
    if (!(await trimSound(forId, lastWordsMs))) return
    if (useMeetingStore.getState().id !== forId) return
    set({ duration: Math.floor(lastWordsMs / 1000) })
    persist()
    offerUndo(forId, before, sound)
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
      answers: [],
      overview: null,
      notes: '',
      names: {},
      title: '',
      folder: inFolder,
      duration: 0,
      kept: null,
      times: [],
    })
  }
  // Ctrl or Cmd with F finds in the text of the meeting open, in place of the web view's
  // own find.
  useEffect(() => {
    const press = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== 'f') return
      if (!finder.current) return
      event.preventDefault()
      event.stopPropagation()
      setTab('text')
      finder.current.focus()
    }
    window.addEventListener('keydown', press, true)
    return () => window.removeEventListener('keydown', press, true)
  }, [])
  const closeRef = useRef(close)
  closeRef.current = close
  const detailRef = useRef(false)
  // The meetings page chosen again in the sidebar: an open meeting is closed, back to the
  // list, unless it is being recorded or worked on.
  useEffect(() => {
    const again = (event: Event) => {
      if ((event as CustomEvent).detail !== 'meeting') return
      if (now() === 'idle' && detailRef.current) closeRef.current()
    }
    window.addEventListener(NAV_AGAIN, again)
    return () => window.removeEventListener(NAV_AGAIN, again)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const removeThis = () => {
    if (id) deleteMeeting(id, title.trim() || plainOf(written ?? '').slice(0, 40))
  }
  const busy = phase === 'recording' || phase === 'finishing'
  const detail = phase === 'idle' && result !== null
  detailRef.current = detail
  const list = phase === 'idle' && result === null
  // Found meetings stay in the order they were found in; the others in the order chosen,
  // under the day they were held on where read by time.
  const inView = (
    found ??
    ordered(
      past.filter((each) => inFolder === '' || (each.folder ?? '') === inFolder),
      order,
    )
  ).filter((summary) => !goingMeetings.includes(summary.id))
  const grouped = !found && (order === 'newest' || order === 'oldest')
  const visible: (
    | { kind: 'day'; day: string; label: string }
    | { kind: 'meeting'; summary: MeetingSummary }
  )[] = grouped
    ? byDay(inView).flatMap((run) => [
        {
          kind: 'day' as const,
          day: run.day,
          label: dayName(run.day, Date.now(), i18n?.language ?? navigator.language, {
            today: t('meeting.today'),
            yesterday: t('meeting.yesterday'),
          }),
        },
        ...run.meetings.map((summary) => ({ kind: 'meeting' as const, summary })),
      ])
    : inView.map((summary) => ({ kind: 'meeting' as const, summary }))
  const startedAt = useMeetingStore.getState().startedAt
  const duration = useMeetingStore.getState().duration
  const item =
    'cursor-pointer border-none bg-transparent px-3 py-1.5 text-left text-[13px] text-text-primary hover:bg-bg-secondary'
  const meetingRow = (summary: MeetingSummary) => (
    <li
      key={summary.id}
      className="group flex items-center gap-3 py-3"
      onContextMenu={(event) =>
        openContextMenu(event, [
          { label: t('contextMenu.open'), onSelect: () => open(summary) },
          {
            label: t('meeting.minutes'),
            disabled: making !== null || tidying !== null,
            onSelect: () => pickTemplate(summary),
          },
          { label: t('meeting.folder'), onSelect: () => setMoving(summary.id) },
          {
            label: t('meeting.deleteMeeting'),
            danger: true,
            onSelect: () => removePast(summary),
          },
        ])
      }
    >
      <button
        type="button"
        onClick={() => open(summary)}
        className="flex min-w-0 flex-1 cursor-pointer flex-col gap-0.5 border-none bg-transparent p-0 text-left"
      >
        <span className="truncate text-[13px] font-medium text-text-primary">
          {summary.title?.trim() || plainOf(summary.preview)}
        </span>
        {found && summary.title?.trim() && (
          <span className="truncate text-[12px] text-text-secondary">{summary.preview}</span>
        )}
        <span className="text-[12px] tabular-nums text-text-tertiary">
          {grouped ? timeOf(summary.started_at) : when(summary.started_at)}
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
        <ScrollText size={15} className={making?.id === summary.id ? 'animate-pulse' : undefined} />
      </button>
      <div ref={moving === summary.id ? movingRef : undefined} className="relative flex">
        <button
          type="button"
          aria-label={t('meeting.folder')}
          title={t('meeting.folder')}
          aria-expanded={moving === summary.id}
          onClick={() => setMoving(moving === summary.id ? null : summary.id)}
          className="cursor-pointer border-none bg-transparent p-1 text-text-tertiary opacity-50 transition-opacity hover:text-text-primary focus-visible:opacity-100 group-hover:opacity-100 aria-expanded:opacity-100"
        >
          <FolderInput size={14} />
        </button>
        {moving === summary.id && (
          <div
            role="menu"
            className="absolute right-0 top-full z-20 mt-1 flex min-w-[180px] flex-col rounded-md border border-border bg-bg-elevated py-1 shadow-lg"
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
              className="mx-2 my-1 h-7 rounded-sm border border-border bg-bg-primary px-2 text-[12px] outline-none focus:border-accent"
            />
          </div>
        )}
      </div>
      <button
        type="button"
        aria-label={t('meeting.deleteMeeting')}
        onClick={() => removePast(summary)}
        className="cursor-pointer border-none bg-transparent p-1 text-text-tertiary opacity-50 transition-opacity hover:text-error focus-visible:opacity-100 group-hover:opacity-100"
      >
        <Trash2 size={14} />
      </button>
    </li>
  )
  return (
    <section className="flex flex-col gap-5">
      {folderOffer && (
        <div
          role="status"
          className="flex flex-wrap items-center gap-2 rounded-md border border-border bg-bg-secondary px-3 py-2 text-[13px]"
        >
          <FolderInput size={14} className="text-text-tertiary" aria-hidden />
          <span className="min-w-0 flex-1">
            {t('meeting.folderOffer', { title: folderOffer.title, folder: folderOffer.folder })}
          </span>
          <button
            type="button"
            className="btn-quiet px-2.5 text-accent"
            onClick={() => acceptFolder(folderOffer)}
          >
            {t('meeting.folderOfferYes')}
          </button>
          <button type="button" className="btn-quiet px-2.5" onClick={() => setFolderOffer(null)}>
            {t('meeting.folderOfferNo')}
          </button>
        </div>
      )}
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
                        className="h-[18px] w-10 rounded-xs border border-border bg-bg-primary px-1 text-[12px] tabular-nums outline-none"
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
            <span className="flex items-center gap-2 text-[22px] font-semibold tabular-nums">
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
              <PenLine size={16} className={going ? 'animate-pulse' : undefined} />
            </button>
            <CopyButton text={() => plainOf(shared)} label={t('meeting.copy')} size={16} />
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
            <div ref={moreRef} className="relative flex">
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
                  className="absolute right-0 top-full z-20 mt-1 flex min-w-[168px] flex-col rounded-md border border-border bg-bg-elevated py-1 shadow-lg"
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
                        onClick={() => void saveMeetingAudio(id, t('meeting.audioFileName'), kept)}
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
                className="h-8 rounded-md border border-border bg-bg-primary px-2 text-[12px] text-text-primary outline-none"
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
                    className="flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1 text-[12px]"
                  >
                    {name}
                    <button
                      type="button"
                      aria-label={t('meeting.forgetVoice', { name })}
                      onClick={() =>
                        void Promise.resolve(forgetSpeaker(name))
                          .then((all) => setVoices(all ?? []))
                          .catch(() => toast.error(t('meeting.error.voice')))
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
                    className="rounded-xs outline-none focus:bg-bg-secondary"
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
          {phase === 'recording' && paused && quietPaused !== null && (
            <div
              role="alert"
              className="mt-3 flex flex-wrap items-center gap-2 rounded-md border border-border bg-bg-secondary/40 px-3 py-2 text-[13px] text-text-primary"
            >
              <span className="flex-1">{t('meeting.quietPaused', { count: quietPaused })}</span>
              <button type="button" className="btn-quiet px-2.5" onClick={pause}>
                {t('meeting.resume')}
              </button>
              <button type="button" className="btn-quiet px-2.5" onClick={() => stop(false)}>
                {t('meeting.stop')}
              </button>
            </div>
          )}
          {phase === 'recording' &&
            !paused &&
            level.seconds >= MIC_DEAD_AFTER &&
            loudest < MIC_DEAD_LEVEL && (
              <p role="alert" className="mt-3 text-[13px] text-error">
                {t('meeting.micSilent')}
              </p>
            )}
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
                className="ml-3 inline-flex h-6 w-6 items-center justify-center rounded-sm align-middle text-text-secondary hover:bg-bg-secondary hover:text-text-primary disabled:opacity-40"
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
      {list && past.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5">
          {folders.length > 0 &&
            ['', ...folders].map((name) =>
              renaming === name && name !== '' ? (
                <input
                  key={name}
                  autoFocus
                  defaultValue={name}
                  maxLength={40}
                  aria-label={t('meeting.renameFolder')}
                  onBlur={(event) => {
                    if (event.currentTarget.dataset.cancelled !== '1')
                      renameFolder(name, event.currentTarget.value)
                  }}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') renameFolder(name, event.currentTarget.value)
                    if (event.key === 'Escape') {
                      // Leaving the field as it goes would otherwise take the name typed.
                      event.currentTarget.dataset.cancelled = '1'
                      setRenaming(null)
                    }
                  }}
                  className="h-7 w-36 rounded-full border border-accent bg-transparent px-3 text-[12px] text-text-primary outline-none"
                />
              ) : (
                <button
                  key={name}
                  type="button"
                  aria-pressed={inFolder === name}
                  onClick={() => pickFolder(name)}
                  onDoubleClick={() => name !== '' && setRenaming(name)}
                  onContextMenu={(event) =>
                    name !== '' &&
                    openContextMenu(event, [
                      { label: t('meeting.renameFolder'), onSelect: () => setRenaming(name) },
                      {
                        label: t('meeting.removeFolder'),
                        danger: true,
                        onSelect: () => removeFolder(name),
                      },
                    ])
                  }
                  className="flex h-7 cursor-pointer items-center gap-1.5 rounded-full border border-border bg-transparent px-3 text-[12px] text-text-secondary hover:text-text-primary aria-pressed:border-accent aria-pressed:text-accent"
                >
                  {name !== '' && <Folder size={12} aria-hidden />}
                  {name === '' ? t('meeting.allFolders') : name}
                </button>
              ),
            )}
          {inFolder !== '' && renaming === null && (
            <>
              <button
                type="button"
                aria-label={t('meeting.renameFolder')}
                title={t('meeting.renameFolder')}
                onClick={() => setRenaming(inFolder)}
                className="btn-quiet px-1.5"
              >
                <Pencil size={13} />
              </button>
              <button
                type="button"
                aria-label={t('meeting.removeFolder')}
                title={t('meeting.removeFolder')}
                onClick={() => removeFolder(inFolder)}
                className="btn-quiet px-1.5 hover:text-error"
              >
                <FolderMinus size={13} />
              </button>
            </>
          )}
          <label className="ml-auto flex items-center gap-1.5 text-[12px] text-text-tertiary">
            <ArrowDownUp size={12} aria-hidden />
            <select
              aria-label={t('meeting.order')}
              value={order}
              onChange={(event) => chooseOrder(event.target.value as MeetingOrder)}
              className="cursor-pointer border-none bg-transparent text-[12px] text-text-secondary outline-none"
            >
              {MEETING_ORDERS.map((each) => (
                <option key={each} value={each}>
                  {t(`meeting.orderBy.${each}`)}
                </option>
              ))}
            </select>
          </label>
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
          {picking.meeting && (
            <div className="flex flex-wrap items-center gap-2">
              <MinutesForm />
            </div>
          )}
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
            {visible.slice(0, shownCount).map((entry) =>
              entry.kind === 'day' ? (
                <li
                  key={`day-${entry.day}`}
                  className="pb-1.5 pt-4 text-[11px] font-medium text-text-tertiary first:pt-1"
                >
                  {entry.label}
                </li>
              ) : (
                meetingRow(entry.summary)
              ),
            )}
            {inView.length === 0 && (
              <li className="py-10 text-center text-[13px] text-text-tertiary">
                {t('meeting.noMatches')}
              </li>
            )}
            {visible.length > shownCount && (
              <li className="flex justify-center py-3">
                <button
                  type="button"
                  onClick={() => setShownCount((count) => count + PAGE)}
                  className="btn-quiet px-3 text-[12px]"
                >
                  {t('meeting.showMore', {
                    count: visible.slice(shownCount).filter((each) => each.kind === 'meeting')
                      .length,
                  })}
                </button>
              </li>
            )}
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
          <div
            role="tablist"
            className="-mb-1 flex gap-6 border-b border-border"
            onKeyDown={(event) => {
              const all = ['text', 'overview', 'minutes'] as const
              const by = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0
              if (!by) return
              event.preventDefault()
              const next = all[(all.indexOf(tab) + by + all.length) % all.length]
              setTab(next)
              event.currentTarget.querySelector<HTMLButtonElement>(`[data-tab="${next}"]`)?.focus()
            }}
          >
            {(['text', 'overview', 'minutes'] as const).map((each) => (
              <button
                key={each}
                type="button"
                role="tab"
                data-tab={each}
                tabIndex={tab === each ? 0 : -1}
                aria-selected={tab === each}
                onClick={() => setTab(each)}
                className={`-mb-px cursor-pointer border-0 border-b-2 border-solid bg-transparent px-0 pb-2 text-[13px] ${
                  tab === each
                    ? 'border-text-primary font-medium text-text-primary'
                    : 'border-transparent text-text-secondary hover:text-text-primary'
                }`}
              >
                {t(
                  each === 'text'
                    ? 'meeting.tabText'
                    : each === 'overview'
                      ? 'meeting.tabOverview'
                      : 'meeting.tabMinutes',
                )}
              </button>
            ))}
          </div>
          <div hidden={tab !== 'text'} className="flex flex-col gap-4">
            {kept && (
              <MeetingAudio
                key={`${id ?? ''}#${soundVersion}`}
                ref={player}
                src={
                  soundVersion > 0 ? `${audioSource(kept)}?v=${soundVersion}` : audioSource(kept)
                }
                onPosition={setPosition}
              />
            )}
            {kept &&
              id &&
              (cuttingAt !== null ? (
                <div className="flex flex-wrap items-center gap-2 text-[12px] text-text-secondary">
                  <span>{t('meeting.cutFromHereAsk', { time: clock(cuttingAt) })}</span>
                  <button
                    type="button"
                    className="btn-quiet px-2.5 text-error"
                    onClick={() => {
                      const at = cuttingAt
                      setCuttingAt(null)
                      void cutAtPlayer(at * 1000)
                    }}
                  >
                    {t('meeting.cutFromHereConfirm')}
                  </button>
                  <button
                    type="button"
                    className="btn-quiet px-2.5"
                    onClick={() => setCuttingAt(null)}
                  >
                    {t('common.cancel')}
                  </button>
                </div>
              ) : (
                <button
                  type="button"
                  className="btn-quiet w-fit px-2.5 text-[12px] disabled:opacity-50"
                  disabled={cutLineAt(Math.floor(position) * 1000) === null}
                  title={t('meeting.cutFromHereHint')}
                  onClick={() => setCuttingAt(Math.floor(position))}
                >
                  <Scissors size={12} />
                  {t('meeting.cutFromHere', { time: clock(Math.floor(position)) })}
                </button>
              ))}
            {kept &&
              lastWordsMs !== null &&
              duration * 1000 > lastWordsMs + SILENT_TAIL_MS &&
              (cuttingTail ? (
                <div className="flex flex-wrap items-center gap-2 text-[12px] text-text-secondary">
                  <span>
                    {t('meeting.cutSilentTailAsk', { time: clock(Math.floor(lastWordsMs / 1000)) })}
                  </span>
                  <button
                    type="button"
                    className="btn-quiet px-2.5 text-error"
                    onClick={() => {
                      setCuttingTail(false)
                      void cutSilentTail()
                    }}
                  >
                    {t('meeting.cutSilentTailConfirm')}
                  </button>
                  <button
                    type="button"
                    className="btn-quiet px-2.5"
                    onClick={() => setCuttingTail(false)}
                  >
                    {t('common.cancel')}
                  </button>
                </div>
              ) : (
                <button
                  type="button"
                  className="btn-quiet w-fit px-2.5 text-[12px]"
                  onClick={() => setCuttingTail(true)}
                >
                  <Scissors size={12} />
                  {t('meeting.cutSilentTail', {
                    minutes: Math.round((duration * 1000 - lastWordsMs) / 60_000),
                  })}
                </button>
              ))}
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
            {namesFound?.id === id && !going && (
              <div
                role="group"
                aria-label={t('meeting.namesFound')}
                className="space-y-1.5 rounded-md border border-border bg-bg-secondary/40 px-3 py-2.5 text-[12px]"
              >
                <div className="text-text-secondary">{t('meeting.namesFound')}</div>
                {namesFound.list.map((found) => {
                  const drop = () => {
                    const list = namesFound.list.filter((other) => other.speaker !== found.speaker)
                    set({ namesFound: list.length > 0 ? { id: namesFound.id, list } : null })
                  }
                  return (
                    <div key={found.speaker} className="flex flex-wrap items-center gap-2">
                      <span style={{ color: speakerColor(found.speaker) }} className="font-medium">
                        {label} {found.speaker} → {found.name}
                      </span>
                      <span
                        className="min-w-0 flex-1 truncate text-text-tertiary"
                        title={found.quote}
                      >
                        “{found.quote}”
                      </span>
                      <button
                        type="button"
                        className="btn-quiet px-2 text-accent"
                        onClick={() => {
                          set({
                            names: {
                              ...useMeetingStore.getState().names,
                              [found.speaker]: found.name,
                            },
                          })
                          nameKept(found.speaker, found.name)
                          drop()
                          persist()
                        }}
                      >
                        {t('meeting.nameTake')}
                      </button>
                      <button type="button" className="btn-quiet px-2" onClick={drop}>
                        {t('meeting.nameLeave')}
                      </button>
                    </div>
                  )
                })}
              </div>
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
                    className="h-8 w-[130px] rounded-md border border-border bg-bg-primary px-2.5 text-[12px] font-medium outline-none placeholder:font-normal placeholder:text-text-tertiary"
                  />
                ))}
              </div>
            )}
            <FindInText
              ref={finder}
              within={textBox}
              version={written ?? ''}
              speakers={speakerNumbers.map((number) => ({
                number,
                name: names[number]?.trim() || `${label} ${number}`,
              }))}
            />
            {!editHintSeen && (
              <p className="m-0 flex items-center gap-2 text-[12px] text-text-tertiary">
                <MousePointerClick size={13} aria-hidden />
                <span className="flex-1">{t('meeting.editHint')}</span>
                <button
                  type="button"
                  aria-label={t('common.close')}
                  onClick={seeEditHint}
                  className="btn-quiet px-1"
                >
                  <X size={12} />
                </button>
              </p>
            )}
            <div ref={textBox}>
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
                onCut={cut}
                cutLabel={t('meeting.cut')}
                cutConfirmLabel={t('meeting.cutConfirm')}
                onCutAfter={id ? (line) => void cutAfter(line) : undefined}
                cutAfterLabel={t('meeting.cutAfter')}
                cutAfterConfirmLabel={
                  kept ? t('meeting.cutAfterConfirmSound') : t('meeting.cutAfterConfirm')
                }
              />
            </div>
          </div>
          <div hidden={tab !== 'overview'}>
            {written !== null && written.trim() !== '' && (
              <MeetingOverviewPanel
                key={id ?? 'new'}
                meetingId={id}
                transcript={written}
                label={label}
                names={names}
                overview={overview}
                active={tab === 'overview'}
                busy={phase !== 'idle' || tidying !== null || making?.id === id}
                onOverview={(made) => {
                  const forId = id
                  const now = useMeetingStore.getState()
                  if (now.id === forId) {
                    set({ overview: made })
                    persist()
                    return
                  }
                  // Another meeting was opened while this one's overview was made: it goes
                  // with its own meeting, if that still says what it was made from.
                  if (!forId) return
                  void Promise.resolve(meetingGet(forId))
                    .then((kept) =>
                      kept && kept.transcript.length === made.chars
                        ? meetingSave({ ...kept, overview: made })
                        : undefined,
                    )
                    .catch(() => {})
                }}
                onSeek={
                  kept
                    ? (seconds) => {
                        setTab('text')
                        player.current?.seek(seconds, true)
                      }
                    : undefined
                }
              />
            )}
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
                answers={answers}
                onAnswers={(next) => {
                  set({ answers: next })
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
