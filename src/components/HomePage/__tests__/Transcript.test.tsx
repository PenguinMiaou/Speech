import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { Transcript } from '../Transcript'

const text = 'Speaker 1  00:10\n周五上线先测试\n\nSpeaker 2  00:20\n好的'
const times = [
  {
    end_ms: 18000,
    chars: 7,
    marks: [
      [10000, 0],
      [15000, 4],
    ].map(([ms, at]) => [at, ms] as [number, number]),
  },
  { end_ms: 21000, chars: 2, marks: [[0, 20000] as [number, number]] },
]

const shown = (props: Partial<Parameters<typeof Transcript>[0]>) =>
  render(<Transcript text={text} names={{}} speakerLabel="Speaker" label="text" {...props} />)

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  Reflect.deleteProperty(document, 'caretRangeFromPoint')
})

describe('the words of a meeting whose times are known', () => {
  it('move the sound to the word that is clicked, not to the start of its paragraph', () => {
    const onSeek = vi.fn()
    shown({ times, onSeek })
    const words = screen.getByText('周五上线先测试')
    Object.assign(document, {
      caretRangeFromPoint: () => ({ startContainer: words.firstChild, startOffset: 4 }),
    })
    fireEvent.click(words)
    expect(onSeek).toHaveBeenCalledWith(15, false)
  })

  it('show how far the sound has got inside the paragraph being played', () => {
    const highlights = new Map<string, { ranges: Range[] }>()
    vi.stubGlobal('CSS', { highlights })
    vi.stubGlobal(
      'Highlight',
      class {
        ranges: Range[]
        constructor(...ranges: Range[]) {
          this.ranges = ranges
        }
      },
    )
    shown({ times, onSeek: vi.fn(), position: 16.5 })
    const [ahead] = highlights.get('ahead')?.ranges ?? []
    expect(ahead?.startContainer.textContent).toBe('周五上线先测试')
    expect(ahead?.startOffset).toBe(5)
    expect(ahead?.endOffset).toBe(7)
  })

  it('move to the start of the paragraph where the times are not known', () => {
    const onSeek = vi.fn()
    shown({ onSeek })
    fireEvent.click(screen.getByText('周五上线先测试'))
    expect(onSeek).toHaveBeenCalledWith(10, false)
  })
})
