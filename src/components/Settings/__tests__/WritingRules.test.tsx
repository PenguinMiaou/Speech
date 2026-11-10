import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { WritingRules } from '../WritingRules'
import { useAppStore } from '../../../stores/appStore'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

describe('WritingRules', () => {
  afterEach(cleanup)

  it('chooses the Chinese script', () => {
    render(<WritingRules />)
    fireEvent.click(screen.getByRole('combobox', { name: 'settings.chineseScript' }))
    fireEvent.click(screen.getByRole('option', { name: 'settings.chineseScriptTraditional' }))
    expect(useAppStore.getState().config.polish_chinese_script).toBe('traditional')
  })

  it('chooses how much punctuation is added', () => {
    render(<WritingRules />)
    expect(screen.getByRole('combobox', { name: 'settings.punctuationMode' }).textContent).toBe(
      'settings.punctuationAuto',
    )
    fireEvent.click(screen.getByRole('combobox', { name: 'settings.punctuationMode' }))
    fireEvent.click(screen.getByRole('option', { name: 'settings.punctuationSpoken' }))
    expect(useAppStore.getState().config.punctuation_mode).toBe('spoken')
  })
})
