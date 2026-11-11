import { describe, expect, it } from 'vitest'
import { formatted, hasFormat, plainOf } from '../meeting-format'

describe('the setting out of put-right words', () => {
  it('tells strong words from the rest', () => {
    expect(formatted('we **ship Friday** then rest')).toEqual([
      { text: 'we ', strong: false, fresh: false },
      { text: 'ship Friday', strong: true, fresh: false },
      { text: ' then rest', strong: false, fresh: false },
    ])
  })

  it('begins each counted point on a line of its own', () => {
    expect(formatted('三步：①备份，②**灰度**，③切换')).toEqual([
      { text: '三步：', strong: false, fresh: false },
      { text: '①备份，', strong: false, fresh: true },
      { text: '②', strong: false, fresh: true },
      { text: '灰度', strong: true, fresh: false },
      { text: '，', strong: false, fresh: false },
      { text: '③切换', strong: false, fresh: true },
    ])
  })

  it('leaves a single counted point in its sentence', () => {
    expect(formatted('见图①即可')).toEqual([{ text: '见图①即可', strong: false, fresh: false }])
  })

  it('drops a mark that has no partner', () => {
    expect(formatted('half **open')).toEqual([{ text: 'half open', strong: false, fresh: false }])
  })

  it('sets the words out the same however often it is asked', () => {
    const text = 'a **b** c'
    expect(hasFormat(text)).toBe(true)
    expect(formatted(text)).toHaveLength(3)
    expect(hasFormat(text)).toBe(true)
    expect(formatted(text)).toHaveLength(3)
  })

  it('reads the same without its marks', () => {
    expect(plainOf('we **ship Friday**, ①a ②b **')).toBe('we ship Friday, ①a ②b ')
    expect(hasFormat('plain words')).toBe(false)
    expect(hasFormat('a **b** c')).toBe(true)
    expect(hasFormat('①a ②b')).toBe(true)
    expect(hasFormat('图①')).toBe(false)
  })
})
