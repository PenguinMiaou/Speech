import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { LogoSelect } from '../shared/LogoSelect'

const OPTIONS = [
  { value: 'groq', label: 'Groq', logo: 'groq' },
  { value: 'openai', label: 'OpenAI', logo: 'openai' },
  { value: 'custom', label: 'Custom', logo: null },
]

function renderSelect(onChange = vi.fn()) {
  render(<LogoSelect label="Provider" value="groq" options={OPTIONS} onChange={onChange} />)
  return { onChange, trigger: screen.getByRole('combobox', { name: 'Provider' }) }
}

describe('LogoSelect', () => {
  afterEach(cleanup)

  it('shows the chosen option with its logo', () => {
    const { trigger } = renderSelect()
    expect(trigger.textContent).toContain('Groq')
    expect(trigger.querySelector('img')).toBeTruthy()
    expect(screen.queryByRole('listbox')).toBeNull()
  })

  it('lists every option with a logo tile when opened', () => {
    const { trigger } = renderSelect()
    fireEvent.click(trigger)
    const options = screen.getAllByRole('option')
    expect(options.map((option) => option.textContent)).toEqual(['Groq', 'OpenAI', 'Custom'])
    expect(options[0].getAttribute('aria-selected')).toBe('true')
    expect(options[1].querySelector('img')).toBeTruthy()
  })

  it('picks an option with a click and closes', () => {
    const { trigger, onChange } = renderSelect()
    fireEvent.click(trigger)
    fireEvent.click(screen.getByRole('option', { name: 'OpenAI' }))
    expect(onChange).toHaveBeenCalledWith('openai')
    expect(screen.queryByRole('listbox')).toBeNull()
  })

  it('moves with the arrow keys and picks with Enter', () => {
    const { trigger, onChange } = renderSelect()
    fireEvent.keyDown(trigger, { key: 'ArrowDown' })
    expect(screen.getByRole('listbox')).toBeTruthy()
    fireEvent.keyDown(trigger, { key: 'ArrowDown' })
    fireEvent.keyDown(trigger, { key: 'Enter' })
    expect(onChange).toHaveBeenCalledWith('openai')
  })

  it('closes on Escape without changing anything', () => {
    const { trigger, onChange } = renderSelect()
    fireEvent.click(trigger)
    fireEvent.keyDown(trigger, { key: 'Escape' })
    expect(screen.queryByRole('listbox')).toBeNull()
    expect(onChange).not.toHaveBeenCalled()
  })

  it('leaves out the icon tile for options that have no logo', () => {
    render(
      <LogoSelect
        label="Language"
        value="zh"
        options={[
          { value: 'zh', label: '中文' },
          { value: 'en', label: 'English' },
        ]}
        onChange={vi.fn()}
      />,
    )
    const trigger = screen.getByRole('combobox', { name: 'Language' })
    expect(trigger.textContent).toBe('中文')
    expect(trigger.querySelector('[data-option-icon]')).toBeNull()
    fireEvent.click(trigger)
    const english = screen.getByRole('option', { name: 'English' })
    expect(english.querySelector('[data-option-icon]')).toBeNull()
  })

  it('keeps the icon tile for options that have one', () => {
    const { trigger } = renderSelect()
    expect(trigger.querySelector('[data-option-icon]')).toBeTruthy()
  })

  it('is shorter with smaller text in compact form', () => {
    render(
      <LogoSelect
        label="Target"
        value="en"
        options={[{ value: 'en', label: 'English' }]}
        onChange={vi.fn()}
        compact
      />,
    )
    const trigger = screen.getByRole('combobox', { name: 'Target' })
    expect(trigger.className).toContain('h-8')
    expect(trigger.className).toContain('text-[12px]')
  })

  it('closes on a click outside', () => {
    const { trigger } = renderSelect()
    fireEvent.click(trigger)
    fireEvent.mouseDown(document.body)
    expect(screen.queryByRole('listbox')).toBeNull()
  })
})
