import type { MeetingSummary } from './tauri'

/** The orders the list of meetings can be read in. */
export type MeetingOrder = 'newest' | 'oldest' | 'longest' | 'title'
export const MEETING_ORDERS: MeetingOrder[] = ['newest', 'oldest', 'longest', 'title']

const titleOf = (meeting: MeetingSummary) => meeting.title?.trim() || meeting.preview

/** The meetings in the order asked for; the newest first among equals. */
export function ordered(meetings: MeetingSummary[], order: MeetingOrder): MeetingSummary[] {
  const byTime = (a: MeetingSummary, b: MeetingSummary) => b.started_at - a.started_at
  const rule: Record<MeetingOrder, (a: MeetingSummary, b: MeetingSummary) => number> = {
    newest: byTime,
    oldest: (a, b) => -byTime(a, b),
    longest: (a, b) => (b.duration_s ?? 0) - (a.duration_s ?? 0) || byTime(a, b),
    title: (a, b) => titleOf(a).localeCompare(titleOf(b)) || byTime(a, b),
  }
  return [...meetings].sort(rule[order])
}

/** The local day `time` falls on, as YYYY-MM-DD. */
export function dayOf(time: number): string {
  const date = new Date(time)
  const two = (n: number) => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${two(date.getMonth() + 1)}-${two(date.getDate())}`
}

/** The meetings, in their order, in runs of the same local day. */
export function byDay(meetings: MeetingSummary[]): { day: string; meetings: MeetingSummary[] }[] {
  const runs: { day: string; meetings: MeetingSummary[] }[] = []
  for (const meeting of meetings) {
    const day = dayOf(meeting.started_at)
    const last = runs[runs.length - 1]
    if (last?.day === day) last.meetings.push(meeting)
    else runs.push({ day, meetings: [meeting] })
  }
  return runs
}

/** A day's name as a heading: today, yesterday, or the date with its weekday, in `language`. */
export function dayName(
  day: string,
  now: number,
  language: string,
  words: { today: string; yesterday: string },
): string {
  if (day === dayOf(now)) return words.today
  if (day === dayOf(now - 86_400_000)) return words.yesterday
  const [year, month, date] = day.split('-').map(Number)
  const at = new Date(year, month - 1, date)
  const sameYear = year === new Date(now).getFullYear()
  return new Intl.DateTimeFormat(language, {
    weekday: 'short',
    month: 'long',
    day: 'numeric',
    ...(sameYear ? {} : { year: 'numeric' }),
  }).format(at)
}
