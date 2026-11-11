import { create } from 'zustand'
import i18n from 'i18next'
import {
  meetingAudioKeep,
  meetingLearn,
  meetingRecover,
  meetingSave,
  meetingSpeakersAgain,
  meetingTitle,
  meetingTranslate,
  meetingWordTimes,
  type FileTranscribeProgress,
  meetingGet,
  meetingPolish,
  meetingSpeakersCheck,
  onFileTranscribeProgress,
  onMeetingPolishing,
  onMeetingTerms,
  type MeetingTerm,
  onFileTranscribeWritten,
  onMeetingError,
  onMeetingInterim,
  onMeetingNames,
  onMeetingSegment,
  onMeetingTranslating,
  speakerConfirm,
  type MeetingSegment,
} from '../lib/tauri'
import {
  speakersOf,
  spokenLines,
  withLines,
  withSpeakersChecked,
  addSegment,
  addTranslation,
  meetingText,
  spansOf,
  speakersAgain,
  timesOf,
  whoIsMe,
  withMarks,
  withParagraphText,
  withSide,
  withSpeakers,
  type Paragraph,
  type Sides,
  type Times,
} from '../lib/meeting-text'

export type MeetingPhase = 'idle' | 'recording' | 'finishing' | 'working'

/** The meeting in hand. It lives here, not in the card, so that a recording goes on and
 * its text is kept while another page is open. */
export interface MeetingState {
  phase: MeetingPhase
  /** How far the file being transcribed has got, what is written out of it so far, and
   * when the work on it began. */
  progress: FileTranscribeProgress | null
  soFar: string
  workingSince: number | null
  /** The meeting being put right in steps: which one, how far, since when, and whether
   * who said what is checked after the words. */
  tidying: { id: string; fraction: number; since: number; steps: number } | null
  /** The text of a meeting as it was before it was put right, to show what changed and
   * to go back to. */
  unpolished: { id: string; text: string } | null
  /** The terms the polish model could not be sure how to spell, to be asked about once. */
  terms: { id: string; list: MeetingTerm[] } | null
  paragraphs: Paragraph[]
  /** The text of a transcribed file. */
  text: string | null
  /** True once a recording has ended and `paragraphs` is its text. */
  recorded: boolean
  /** Where the latest recording's audio is kept on disk. */
  kept: string | null
  error: string | null
  /** What the user typed during the meeting. */
  notes: string
  /** Names the user gave to speaker numbers. */
  names: Record<number, string>
  /** The minutes written for this meeting. */
  minutes: string | null
  /** Set once the meeting has text and is kept among the past meetings. */
  id: string | null
  startedAt: number
  /** True while the speakers' voices of this meeting are still at hand, which they are
   * not for a meeting opened from the past ones. */
  voicesAtHand: boolean
  /** The language each piece is translated into as it arrives, by the polish model. Null
   * when the meeting is not translated, and then no text leaves the machine while recording. */
  translateTo: string | null
  /** The name the user gave the meeting. */
  title: string
  /** How long the recording ran, in seconds; 0 when not known. */
  duration: number
  /** A guess at the words being said now, shown until the settled words replace it. */
  interim: string
  /** The paragraph the user is changing while the recording runs, if any. */
  editing: number | null
  /** The translation being written now, as far as it has got, and the paragraph it is of. */
  translating: string
  translatingAt: number | null
  /** The title the app gave, with the meeting it gave it to; a title the user typed differs. */
  titleFrom: string
  /** True while the speakers are told again from the whole sound. */
  sorting: boolean
  /** How much each speaker said into the microphone and how much came from the computer. */
  sides: Sides
  /** The speaker taken to be the user, who was given the name for that. */
  me: number | null
  /** When the words under each heading of `text` were said; empty when not known. */
  times: Times[]
}

/** A title as the app gave it to a meeting, to tell it later from one the user typed. */
const given = (id: string, title: string) => `${id}\n${title}`

export const initialMeeting: MeetingState = {
  phase: 'idle',
  progress: null,
  soFar: '',
  workingSince: null,
  tidying: null,
  unpolished: null,
  terms: null,
  paragraphs: [],
  text: null,
  recorded: false,
  kept: null,
  error: null,
  notes: '',
  names: {},
  minutes: null,
  id: null,
  startedAt: 0,
  voicesAtHand: false,
  translateTo: null,
  interim: '',
  title: '',
  duration: 0,
  editing: null,
  translating: '',
  translatingAt: null,
  titleFrom: '',
  sorting: false,
  sides: {},
  me: null,
  times: [],
}

export const useMeetingStore = create<MeetingState>(() => initialMeeting)

/** Keeps the meeting in hand among the past meetings, once it has text. */
export function persistMeeting(speakerLabel: string): Promise<void> {
  const state = useMeetingStore.getState()
  const transcript =
    state.text ?? (state.recorded ? meetingText(state.paragraphs, speakerLabel) : null)
  if (!transcript?.trim()) return Promise.resolve()
  let { id, startedAt } = state
  if (!id) {
    startedAt = Date.now()
    id = `m-${startedAt}`
    useMeetingStore.setState({ id, startedAt })
  }
  const named = Object.fromEntries(
    Object.entries(state.names).filter(([, name]) => name.trim() !== ''),
  )
  if (state.voicesAtHand && Object.keys(named).length > 0) {
    void Promise.resolve(speakerConfirm(named)).catch(() => {})
  }
  return Promise.resolve(
    meetingSave({
      id,
      started_at: startedAt,
      transcript,
      notes: state.notes,
      minutes: state.minutes ?? '',
      names: state.names,
      title: state.title.trim(),
      duration_s: state.duration,
      times: state.times,
    }),
  )
    .then(() => {
      // The recording that has just ended goes with the meeting written from it.
      if (!state.recorded) return
      return Promise.resolve(meetingAudioKeep(id)).then((kept) => {
        if (kept && useMeetingStore.getState().id === id) useMeetingStore.setState({ kept })
      })
    })
    .catch(() => {})
}

/** Names the meeting in hand after what was said in it, unless it has a name already.
 * A meeting that cannot be named stays unnamed. */
export function nameMeeting(speakerLabel: string): Promise<void> {
  const state = useMeetingStore.getState()
  const transcript =
    state.text ?? (state.recorded ? meetingText(state.paragraphs, speakerLabel) : null)
  const id = state.id
  if (!id || state.title.trim() || !transcript?.trim()) return Promise.resolve()
  return Promise.resolve(meetingTitle(transcript))
    .then((title) => {
      const now = useMeetingStore.getState()
      // The user may have named it, or opened another meeting, in the meantime.
      if (!title?.trim() || now.id !== id || now.title.trim()) return
      useMeetingStore.setState({ title: title.trim(), titleFrom: given(id, title.trim()) })
      return persistMeeting(speakerLabel)
    })
    .catch(() => {})
}

/** Names the meeting again after its minutes, which say what it was about better than a
 * reading of the words. A name the user typed stands. */
export function nameFromMinutes(speakerLabel: string, minutes: string): Promise<void> {
  const state = useMeetingStore.getState()
  const id = state.id
  const ours = (title: string) => !title.trim() || (id && state.titleFrom === given(id, title))
  if (!id || !minutes.trim() || !ours(state.title)) return Promise.resolve()
  const before = state.title
  return Promise.resolve(meetingTitle(minutes))
    .then((title) => {
      const now = useMeetingStore.getState()
      if (!title?.trim() || now.id !== id || now.title !== before) return
      useMeetingStore.setState({ title: title.trim(), titleFrom: given(id, title.trim()) })
      return persistMeeting(speakerLabel)
    })
    .catch(() => {})
}

/** Tells again who spoke in the meeting just recorded, from the whole of its kept sound,
 * and puts that in the paragraphs and in the headings of the text. */
export function tellSpeakersAgain(speakerLabel: string): Promise<void> {
  const state = useMeetingStore.getState()
  const id = state.id
  const told = state.paragraphs.some((paragraph) => paragraph.speaker !== null)
  if (!id || !state.recorded || !state.kept || !told) return Promise.resolve()
  useMeetingStore.setState({ sorting: true })
  return Promise.resolve(meetingSpeakersAgain(id))
    .then((turns) => {
      const now = useMeetingStore.getState()
      if (!turns?.length || now.id !== id || now.text === null) return
      const speakers = speakersAgain(now.paragraphs, turns)
      const text = withSpeakers(now.text, speakerLabel, speakers)
      if (text === now.text) return
      useMeetingStore.setState({
        text,
        paragraphs: now.paragraphs.map((each, index) => ({ ...each, speaker: speakers[index] })),
      })
      return persistMeeting(speakerLabel)
    })
    .catch(() => {})
    .finally(() => useMeetingStore.setState({ sorting: false }))
}

/** Hears the meeting just recorded again for when each of its words was said, and keeps
 * that with it. Without the model for it, or when the words were changed meanwhile,
 * the times stay as they were. */
export function timeWords(speakerLabel: string): Promise<void> {
  const state = useMeetingStore.getState()
  const { id, text } = state
  if (!id || !state.recorded || !state.kept || !text) return Promise.resolve()
  const spans = spansOf(text, speakerLabel, state.times, state.duration)
  if (spans.length === 0) return Promise.resolve()
  useMeetingStore.setState({ sorting: true })
  return Promise.resolve(meetingWordTimes(id, spans))
    .then((found) => {
      const now = useMeetingStore.getState()
      if (!found || now.id !== id || now.text !== text) return
      const times = withMarks(text, speakerLabel, now.times, now.duration, found)
      useMeetingStore.setState({ times })
      return persistMeeting(speakerLabel)
    })
    .catch(() => {})
    .finally(() => useMeetingStore.setState({ sorting: false }))
}

/** Where the notes of the recording still running are kept, in case it is cut short. */
const NOTES_DRAFT = 'meeting.notesDraft'

function notesDraft(): string {
  try {
    return localStorage.getItem(NOTES_DRAFT) ?? ''
  } catch {
    return ''
  }
}

let lookedForOne = false

/** Brings back a recording that was cut short the last time the app ran, for instance by
 * a crash: what was written down of it becomes the meeting in hand, with the notes typed
 * during it, and its sound is kept with it. True when one was brought back. */
export function recoverMeeting(speakerLabel: string): Promise<boolean> {
  if (lookedForOne) return Promise.resolve(false)
  lookedForOne = true
  return Promise.resolve(meetingRecover())
    .then((found) => {
      if (!found || useMeetingStore.getState().phase !== 'idle') return false
      const paragraphs = found.segments.reduce<Paragraph[]>(
        (all, segment) => addSegment(all, segment),
        [],
      )
      useMeetingStore.setState({
        ...initialMeeting,
        paragraphs,
        // A recording nothing was written down of is still kept, to be transcribed again.
        text: meetingText(paragraphs, speakerLabel) || '…',
        recorded: true,
        duration: found.seconds,
        notes: notesDraft(),
        times: timesOf(paragraphs),
      })
      return true
    })
    .catch(() => false)
}

/** The name the user's own voice goes by until they type another. */
const myName = () => i18n.t('meeting.me')

/** Counts which side a piece came from and names the speaker who turns out to be the
 * user. A name that was typed, or known from the voice, stands. */
function tellMe(segment: MeetingSegment) {
  const state = useMeetingStore.getState()
  const sides = withSide(state.sides, segment)
  if (sides === state.sides) return
  const me = whoIsMe(sides)
  const names = { ...state.names }
  if (state.me !== null && state.me !== me && names[state.me] === myName()) delete names[state.me]
  if (me !== null && !names[me]?.trim()) names[me] = myName()
  useMeetingStore.setState({ sides, me, names })
}

let translating: Promise<void> = Promise.resolve()

/** Takes in one more piece of the meeting and, when asked to, has it translated. Pieces are
 * translated one after another so that they land in the order they were said. */
export function takeSegment(segment: MeetingSegment): Promise<void> {
  const { paragraphs: before, editing } = useMeetingStore.getState()
  // A paragraph the user is changing is left alone; what follows starts a new one.
  const paragraphs = addSegment(before, segment, editing === before.length - 1)
  if (paragraphs === before) return Promise.resolve()
  useMeetingStore.setState({ paragraphs, interim: '' })
  tellMe(segment)
  const target = useMeetingStore.getState().translateTo
  if (!target) return Promise.resolve()
  const index = paragraphs.length - 1
  translating = translating.then(() => {
    useMeetingStore.setState({ translatingAt: index, translating: '' })
    return Promise.resolve(meetingTranslate(segment.text.trim(), target))
      .then((translated) =>
        useMeetingStore.setState((state) => ({
          paragraphs: addTranslation(state.paragraphs, index, translated ?? ''),
        })),
      )
      .catch(() => useMeetingStore.setState({ error: 'meeting.translateFailed' }))
      .finally(() => useMeetingStore.setState({ translatingAt: null, translating: '' }))
  })
  return translating
}

/** Resolves once every piece waiting to be translated has been. */
export function whenTranslated(): Promise<void> {
  return translating
}

/** Tells the dictionary, and the recognizer of a running recording, which words the user
 * put right. */
export function learnFrom(before: string, after: string) {
  if (before.trim() === after.trim()) return
  void Promise.resolve(meetingLearn(before, after)).catch(() => {})
}

/** Takes the words of a paragraph as the user corrected them during the recording. */
export function editParagraph(index: number, text: string) {
  const before = useMeetingStore.getState().paragraphs
  const paragraphs = withParagraphText(before, index, text)
  if (paragraphs !== before) {
    useMeetingStore.setState({ paragraphs })
    learnFrom(before[index].text, paragraphs[index].text)
  }
}

let listening = false

/** Starts taking in what the recorder reports. Once is enough for the life of the window. */
export function listenToMeeting() {
  if (listening) return
  listening = true
  void Promise.resolve(onMeetingSegment((segment) => void takeSegment(segment))).catch(() => {})
  void Promise.resolve(
    onMeetingInterim((interim) => {
      // A guess that arrives after the recording has ended has nothing left to stand in for.
      if (useMeetingStore.getState().phase === 'recording') useMeetingStore.setState({ interim })
    }),
  ).catch(() => {})
  void Promise.resolve(
    onMeetingTranslating((text) => {
      // Only a translation that is awaited is shown as it is written.
      if (useMeetingStore.getState().translatingAt !== null) {
        useMeetingStore.setState({ translating: text })
      }
    }),
  ).catch(() => {})
  void Promise.resolve(
    onMeetingNames((known) =>
      useMeetingStore.setState((state) => {
        const names = { ...state.names }
        for (const [number, name] of Object.entries(known ?? {})) {
          // A name the user typed stands; a voice that is known goes by its own name.
          const has = names[Number(number)]?.trim()
          if (!has || (Number(number) === state.me && has === myName())) {
            names[Number(number)] = name
          }
        }
        return { names }
      }),
    ),
  ).catch(() => {})
  void Promise.resolve(
    onMeetingError(() => useMeetingStore.setState({ error: 'meeting.error.failed' })),
  ).catch(() => {})
  // A file goes on being transcribed while another page is open.
  void Promise.resolve(
    onFileTranscribeProgress((progress) => useMeetingStore.setState({ progress })),
  ).catch(() => {})
  void Promise.resolve(
    onFileTranscribeWritten((soFar) => useMeetingStore.setState({ soFar })),
  ).catch(() => {})
  void Promise.resolve(
    onMeetingPolishing(({ step, done, total }) =>
      useMeetingStore.setState(({ tidying }) => {
        if (!tidying || total <= 0) return {}
        const before = step === 'speakers' ? 1 : 0
        return { tidying: { ...tidying, fraction: (before + done / total) / tidying.steps } }
      }),
    ),
  ).catch(() => {})
  void Promise.resolve(
    onMeetingTerms((list) =>
      useMeetingStore.setState(({ tidying }) =>
        tidying && list.length > 0 ? { terms: { id: tidying.id, list } } : {},
      ),
    ),
  ).catch(() => {})
}

/** Puts a meeting right in steps: first the words that were misheard, then, where the
 * speakers are told apart, who said what by the sense of it. It goes on while another
 * page or meeting is open, and its result is kept with the meeting either way. */
export function tidyMeeting(speakerLabel: string, source?: string): Promise<void> {
  const state = useMeetingStore.getState()
  const text = source ?? state.text
  const id = state.id
  if (!text?.trim() || !id || state.tidying) return Promise.resolve()
  const at = spokenLines(text, speakerLabel)
  const lines = text.split('\n')
  const speakers = speakersOf(text, speakerLabel)
  const told = speakers.some((speaker) => speaker !== null)
  useMeetingStore.setState({
    tidying: { id, fraction: 0, since: Date.now(), steps: told ? 2 : 1 },
    error: null,
  })
  return Promise.resolve(meetingPolish(at.map((index) => lines[index])))
    .then((better) => {
      const worded = withLines(text, at, better ?? [])
      if (!told) return worded
      const now = worded.split('\n')
      const pieces = at.map((line, order) => ({ speaker: speakers[order] ?? 0, text: now[line] }))
      // The words that were put right stand even when the speakers could not be checked.
      return Promise.resolve(meetingSpeakersCheck(pieces))
        .then((checked) => (checked ? withSpeakersChecked(worded, speakerLabel, checked) : worded))
        .catch(() => worded)
    })
    .then((next) => {
      if (next === text) return
      const now = useMeetingStore.getState()
      if (now.id === id) {
        // Only a meeting that is still as it was sent off takes the answer.
        if (now.text !== text) return
        useMeetingStore.setState({ text: next, unpolished: { id, text } })
        return persistMeeting(speakerLabel)
      }
      return Promise.resolve(meetingGet(id)).then((kept) => {
        if (!kept || kept.transcript !== text) return
        useMeetingStore.setState({ unpolished: { id, text } })
        return meetingSave({ ...kept, transcript: next })
      })
    })
    .catch((reason) => {
      if (useMeetingStore.getState().id !== id) return
      useMeetingStore.setState({
        error: String(reason).includes('llm_not_configured')
          ? 'meeting.error.llm_not_configured'
          : 'meeting.polishFailed',
      })
    })
    .finally(() => useMeetingStore.setState({ tidying: null }))
}

// The notes of a running recording are kept from moment to moment, so that a recording
// that is cut short is brought back with them.
let draftKept = ''
useMeetingStore.subscribe((state) => {
  if (state.phase !== 'recording' || state.notes === draftKept) return
  draftKept = state.notes
  try {
    localStorage.setItem(NOTES_DRAFT, state.notes)
  } catch {
    // Without storage the notes are only lost if the recording is cut short.
  }
})
