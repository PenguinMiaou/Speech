import { describe, expect, it } from 'vitest'
import { changes } from '../word-diff'

describe('changes', () => {
  it('tells the words taken out and the words put in', () => {
    expect(changes('we use tori for this', 'We use Tauri for this.')).toEqual([
      { kind: 'gone', text: 'we' },
      { kind: 'new', text: 'We' },
      { kind: 'same', text: ' use ' },
      { kind: 'gone', text: 'tori' },
      { kind: 'new', text: 'Tauri' },
      { kind: 'same', text: ' for ' },
      { kind: 'gone', text: 'this' },
      { kind: 'new', text: 'this.' },
    ])
  })

  it('gives the new text back whole when its pieces are joined', () => {
    const after = 'So, the bank over there has been talking about it.'
    const parts = changes('the the bank over there. been talking about ACS', after)
    expect(
      parts
        .filter((part) => part.kind !== 'gone')
        .map((part) => part.text)
        .join(''),
    ).toBe(after)
  })

  it('goes by the character where words are written without spaces', () => {
    expect(changes('我们用淘瑞做', '我们用 Tauri 做')).toEqual([
      { kind: 'same', text: '我们用' },
      { kind: 'gone', text: '淘瑞' },
      { kind: 'new', text: ' Tauri ' },
      { kind: 'same', text: '做' },
    ])
  })

  it('finds nothing changed in the same text', () => {
    expect(changes('all the same', 'all the same')).toEqual([
      { kind: 'same', text: 'all the same' },
    ])
  })
})
