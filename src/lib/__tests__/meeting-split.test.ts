import { describe, expect, it } from 'vitest'
import { calendarOf, withParagraphsSplit } from '../meeting-split'

describe('calendarOf', () => {
  it('names the day of the meeting and the days around it with their weekdays', () => {
    const lines = calendarOf(new Date(2026, 10, 11, 15, 30).getTime()).split('\n')
    expect(lines[0]).toBe('The meeting was held on 2026-11-11 Wednesday.')
    expect(lines).toContain('2026-11-04 Wednesday')
    expect(lines).toContain('2026-11-18 Wednesday')
    expect(lines).toContain('2026-12-01 Tuesday')
    expect(lines).toHaveLength(1 + 14 + 1 + 35)
  })
})

describe('withParagraphsSplit', () => {
  it('begins a paragraph of the same speaker where the subject changes, at its time', () => {
    const text = 'S 1  00:10\naaaa ¶ bbbb\n\nS 2  00:30\ncc'
    expect(withParagraphsSplit(text, 'S', [])).toEqual({
      text: 'S 1  00:10\naaaa\n\nS 1  00:20\nbbbb\n\nS 2  00:30\ncc',
      times: [],
      shaped: undefined,
    })
  })

  it('splits the times of the words along with them', () => {
    const text = 'S 1  00:10\naaaa ¶ bbbb\n\nS 2  00:30\ncc'
    const times = [
      {
        end_ms: 30000,
        chars: 8,
        marks: [
          [0, 10000],
          [4, 22000],
          [6, 26000],
        ] as [number, number][],
      },
      { end_ms: 31000, chars: 2, marks: [[0, 30000]] as [number, number][] },
    ]
    const split = withParagraphsSplit(text, 'S', times)
    expect(split.text).toBe('S 1  00:10\naaaa\n\nS 1  00:22\nbbbb\n\nS 2  00:30\ncc')
    expect(split.times).toEqual([
      { end_ms: 22000, chars: 4, marks: [[0, 10000]] },
      {
        end_ms: 30000,
        chars: 4,
        marks: [
          [0, 22000],
          [2, 26000],
        ],
      },
      times[1],
    ])
  })

  it('cuts what was heard at the same places, to be compared piece by piece', () => {
    const text = 'S 1  00:10\naaaa ¶ bbbb\n\nS 2  00:30\ncc'
    const before = 'S 1  00:10\num aaaa and bbbb\n\nS 2  00:30\ncc'
    expect(withParagraphsSplit(text, 'S', [], before).shaped).toBe(
      'S 1  00:10\num aaaa and\n\nS 1  00:20\nbbbb\n\nS 2  00:30\ncc',
    )
  })

  it('splits where speakers are not told, and guesses the time of a last paragraph', () => {
    // Eight characters at four a second: the second half begins a second in.
    expect(withParagraphsSplit('00:10\naaaa ¶ bbbb', 'S', []).text).toBe(
      '00:10\naaaa\n\n00:11\nbbbb',
    )
  })

  it('leaves a text without the mark alone, and drops a mark that stands nowhere', () => {
    const plain = 'S 1  00:10\naaaa\n\nS 2  00:30\ncc'
    expect(withParagraphsSplit(plain, 'S', [])).toEqual({
      text: plain,
      times: [],
      shaped: undefined,
    })
    expect(withParagraphsSplit('S 1  00:10\n¶ aaaa ¶', 'S', []).text).toBe('S 1  00:10\naaaa')
    // A second line under one heading is not a paragraph of its own to split.
    expect(withParagraphsSplit('S 1  00:10\naaaa\nbb ¶ cc', 'S', []).text).toBe(
      'S 1  00:10\naaaa\nbb cc',
    )
  })
})
