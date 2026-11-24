import { describe, expect, it } from 'vitest'
import { byDay, dayName, dayOf, ordered } from '../meeting-list'
import type { MeetingSummary } from '../tauri'

const at = (iso: string) => new Date(iso).getTime()
const meeting = (id: string, started: string, extra: Partial<MeetingSummary> = {}) =>
  ({ id, started_at: at(started), preview: id, has_minutes: false, ...extra }) as MeetingSummary

describe('the list of meetings', () => {
  const all = [
    meeting('a', '2026-05-02T09:00:00', { duration_s: 600, title: 'Weekly' }),
    meeting('b', '2026-05-03T15:00:00', { duration_s: 60, title: 'Annual plan' }),
    meeting('c', '2026-05-03T10:00:00', { duration_s: 600 }),
  ]

  it('is read newest first, oldest first, longest first or by title', () => {
    expect(ordered(all, 'newest').map((each) => each.id)).toEqual(['b', 'c', 'a'])
    expect(ordered(all, 'oldest').map((each) => each.id)).toEqual(['a', 'c', 'b'])
    expect(ordered(all, 'longest').map((each) => each.id)).toEqual(['c', 'a', 'b'])
    expect(ordered(all, 'title').map((each) => each.id)).toEqual(['b', 'c', 'a'])
  })

  it('is grouped by the local day, with today and yesterday named', () => {
    const runs = byDay(ordered(all, 'newest'))
    expect(runs.map((run) => [run.day, run.meetings.length])).toEqual([
      ['2026-05-03', 2],
      ['2026-05-02', 1],
    ])
    const now = at('2026-05-03T20:00:00')
    const words = { today: 'Today', yesterday: 'Yesterday' }
    expect(dayName(dayOf(now), now, 'en', words)).toBe('Today')
    expect(dayName('2026-05-02', now, 'en', words)).toBe('Yesterday')
    expect(dayName('2026-04-28', now, 'en', words)).toBe('Tue, April 28')
    expect(dayName('2025-04-28', now, 'en', words)).toContain('2025')
  })
})
