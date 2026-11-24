import { useRef } from 'react'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { FindInText, findIn } from '../FindInText'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, unknown>) =>
      values ? `${key} ${Object.values(values).join(' ')}` : key,
  }),
}))

afterEach(cleanup)

function Harness() {
  const box = useRef<HTMLDivElement>(null)
  return (
    <>
      <FindInText
        within={box}
        version="1"
        speakers={[
          { number: 1, name: 'Ann' },
          { number: 2, name: 'Bo' },
        ]}
      />
      <div ref={box}>
        <span data-speaker="1">Ann</span>
        <p>The plan is the Plan.</p>
        <span data-speaker="2">Bo</span>
        <p>No plan here, only plans.</p>
      </div>
    </>
  )
}

describe('finding in a meeting text', () => {
  it('finds every place, case not minded', () => {
    const root = document.createElement('div')
    root.innerHTML = '<b>Plan</b> and <i>plan</i>, plANs'
    expect(findIn(root, 'plan').map((range) => range.toString())).toEqual(['Plan', 'plan', 'plAN'])
    expect(findIn(root, '  ')).toEqual([])
  })

  it('counts the places and moves from one to the next, and goes to a speaker', () => {
    const scrolled: string[] = []
    Element.prototype.scrollIntoView = vi.fn(function (this: Element) {
      scrolled.push(this.textContent ?? '')
    })
    render(<Harness />)
    const field = screen.getByLabelText('meeting.findInText')
    fireEvent.change(field, { target: { value: 'plan' } })
    expect(screen.getByText('meeting.foundAt 1 4')).toBeTruthy()
    fireEvent.keyDown(field, { key: 'Enter' })
    expect(screen.getByText('meeting.foundAt 2 4')).toBeTruthy()
    fireEvent.keyDown(field, { key: 'Enter', shiftKey: true })
    fireEvent.keyDown(field, { key: 'Enter', shiftKey: true })
    expect(screen.getByText('meeting.foundAt 4 4')).toBeTruthy()
    fireEvent.change(field, { target: { value: 'nowhere' } })
    expect(screen.getByText('meeting.foundNone')).toBeTruthy()

    scrolled.length = 0
    fireEvent.change(screen.getByLabelText('meeting.jumpToSpeaker'), { target: { value: '2' } })
    expect(scrolled).toEqual(['Bo'])
  })
})
