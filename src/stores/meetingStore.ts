import { create } from 'zustand'
import {
  meetingAudioKeep,
  meetingLearn,
  meetingSave,
  meetingTitle,
  meetingTranslate,
  onMeetingError,
  onMeetingInterim,
  onMeetingNames,
  onMeetingSegment,
  speakerConfirm,
  type MeetingSegment,
} from '../lib/tauri'
import {
  addSegment,
  addTranslation,
  meetingText,
  withParagraphText,
  type Paragraph,
} from '../lib/meeting-text'

export type MeetingPhase = 'idle' | 'recording' | 'finishing' | 'working'

/** The meeting in hand. It lives here, not in the card, so that a recording goes on and
 * its text is kept while another page is open. */
export interface MeetingState {
  phase: MeetingPhase
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
}

export const initialMeeting: MeetingState = {
  phase: 'idle',
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
      useMeetingStore.setState({ title: title.trim() })
      return persistMeeting(speakerLabel)
    })
    .catch(() => {})
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
  const target = useMeetingStore.getState().translateTo
  if (!target) return Promise.resolve()
  const index = paragraphs.length - 1
  translating = translating.then(() =>
    Promise.resolve(meetingTranslate(segment.text.trim(), target))
      .then((translated) =>
        useMeetingStore.setState((state) => ({
          paragraphs: addTranslation(state.paragraphs, index, translated ?? ''),
        })),
      )
      .catch(() => useMeetingStore.setState({ error: 'meeting.translateFailed' })),
  )
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
    onMeetingNames((known) =>
      useMeetingStore.setState((state) => {
        const names = { ...state.names }
        for (const [number, name] of Object.entries(known ?? {})) {
          // A name the user typed stands.
          if (!names[Number(number)]?.trim()) names[Number(number)] = name
        }
        return { names }
      }),
    ),
  ).catch(() => {})
  void Promise.resolve(
    onMeetingError(() => useMeetingStore.setState({ error: 'meeting.error.failed' })),
  ).catch(() => {})
}
