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

describe('the words of a meeting that was put right', () => {
  it('show what was taken out struck through and what was put in marked', () => {
    shown({
      text: 'Speaker 1  00:10\nWe use Tauri here',
      before: 'Speaker 1  00:10\nwe use tori here',
    })
    expect([...document.querySelectorAll('del')].map((gone) => gone.textContent)).toEqual([
      'we',
      'tori',
    ])
    expect([...document.querySelectorAll('ins')].map((fresh) => fresh.textContent)).toEqual([
      'We',
      'Tauri',
    ])
  })

  it('are shown plainly where nothing changed', () => {
    shown({ before: text })
    expect(document.querySelector('del')).toBeNull()
    expect(document.querySelector('ins')).toBeNull()
  })
})

describe('who speaks', () => {
  const three = 'Speaker 1  00:10\na\n\nSpeaker 1  00:20\nb\n\nSpeaker 2  00:30\nc'

  it('is named once while the same person goes on', () => {
    shown({ text: three })
    expect(screen.getAllByText('Speaker 1')).toHaveLength(1)
    expect(screen.getByText('00:20')).toBeTruthy()
  })

  it('is one person where two numbers go by one name', () => {
    shown({ text: three, names: { 1: 'Ann', 2: 'Ann' } })
    expect(screen.getAllByText('Ann')).toHaveLength(1)
  })

  it('can be put right for one paragraph, or for all that speaker said', () => {
    const onSpeaker = vi.fn()
    shown({ text: three, onSpeaker, allLabel: 'all' })
    fireEvent.click(screen.getByRole('button', { name: 'Speaker 2' }))
    expect(screen.queryByRole('menu')).toBeNull()
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Speaker 2' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'Speaker 1' }))
    expect(onSpeaker).toHaveBeenLastCalledWith(6, 1, false)
    expect(screen.queryByRole('menu')).toBeNull()
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Speaker 2' }))
    fireEvent.click(screen.getByRole('menuitemcheckbox', { name: 'all' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'Speaker 3' }))
    expect(onSpeaker).toHaveBeenLastCalledWith(6, 3, true)
  })

  it('can be given a name where they speak', () => {
    const onName = vi.fn()
    shown({ text: three, onSpeaker: vi.fn(), onName, nameLabel: 'name' })
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Speaker 2' }))
    const name = screen.getByRole('textbox', { name: 'name' })
    fireEvent.change(name, { target: { value: 'Ann' } })
    fireEvent.keyDown(name, { key: 'Enter' })
    expect(onName).toHaveBeenCalledWith(2, 'Ann')
    expect(screen.queryByRole('menu')).toBeNull()
  })
})
