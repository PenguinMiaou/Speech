import { describe, expect, it } from 'vitest'
import i18n, { i18nReady } from '../index'

describe('counted messages', () => {
  it('say one thing in the singular and more in the plural', async () => {
    await i18nReady
    const en = i18n.getFixedT('en')
    expect(en('meeting.people', { count: 1 })).toBe('1 speaker')
    expect(en('meeting.people', { count: 3 })).toBe('3 speakers')
    expect(en('meeting.voices', { count: 1 })).toBe('1 voice remembered')
    expect(en('recordingLimits.durationSeconds', { count: 30 })).toBe('30 seconds')
  })

  it('take the Russian forms for one, a few and many', async () => {
    await i18n.loadLanguages('ru')
    const ru = i18n.getFixedT('ru')
    expect(ru('recordingLimits.durationSeconds', { count: 1 })).toBe('1 секунда')
    expect(ru('recordingLimits.durationSeconds', { count: 3 })).toBe('3 секунды')
    expect(ru('recordingLimits.durationSeconds', { count: 30 })).toBe('30 секунд')
  })
})
