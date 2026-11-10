import { describe, expect, it } from 'vitest'
import { diffText } from '../text-diff'

describe('diffText', () => {
  it('marks words dropped from the original', () => {
    expect(diffText('嗯，明天出发', '明天出发')).toEqual([
      { kind: 'removed', text: '嗯，' },
      { kind: 'same', text: '明天出发' },
    ])
  })

  it('marks a replaced word as removed then added', () => {
    expect(diffText('两点出发', '三点出发')).toEqual([
      { kind: 'removed', text: '两' },
      { kind: 'added', text: '三' },
      { kind: 'same', text: '点出发' },
    ])
  })

  it('reports identical text as one unchanged part', () => {
    expect(diffText('abc', 'abc')).toEqual([{ kind: 'same', text: 'abc' }])
  })

  it('keeps an emoji whole', () => {
    expect(diffText('a👍', 'a👎')).toEqual([
      { kind: 'same', text: 'a' },
      { kind: 'removed', text: '👍' },
      { kind: 'added', text: '👎' },
    ])
  })

  it('handles empty text on either side', () => {
    expect(diffText('', 'new')).toEqual([{ kind: 'added', text: 'new' }])
    expect(diffText('old', '')).toEqual([{ kind: 'removed', text: 'old' }])
    expect(diffText('', '')).toEqual([])
  })

  it('does not compare very long text character by character', () => {
    const long = 'x'.repeat(3000)
    expect(diffText(long, `${long}y`)).toEqual([
      { kind: 'removed', text: long },
      { kind: 'added', text: `${long}y` },
    ])
  })
})
