import { describe, expect, it } from 'vitest'
import { dictationStats, formatMinutes } from '../dictation-stats'

const entry = (polished_text: string, duration_ms: number | null) => ({
  polished_text,
  duration_ms,
})

describe('dictationStats', () => {
  it('counts Chinese by character and other text by word', () => {
    expect(dictationStats([entry('今天下午三点开会。', null)]).units).toBe(8)
    expect(dictationStats([entry('See you at three, okay?', null)]).units).toBe(5)
    expect(dictationStats([entry('用 GitHub 发布 v2', null)]).units).toBe(5)
  })

  it('reckons time saved against typing forty units a minute', () => {
    // 400 units would take ten minutes to type; saying them took two.
    const text = '字'.repeat(400)
    expect(dictationStats([entry(text, 120_000)]).savedMinutes).toBe(8)
  })

  it('never reports time lost, and ignores entries of unknown length', () => {
    expect(dictationStats([entry('好', 60_000)]).savedMinutes).toBe(0)
    expect(dictationStats([entry('字'.repeat(400), null)]).savedMinutes).toBe(0)
    expect(dictationStats([])).toEqual({ units: 0, savedMinutes: 0 })
  })

  it('writes minutes as hours once there are enough', () => {
    expect(formatMinutes(45)).toBe('45 min')
    expect(formatMinutes(150)).toBe('2.5 h')
  })
})
