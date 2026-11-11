import { describe, it, expect, vi } from 'vitest'
import { render } from '@testing-library/react'
import { CapsuleRecording } from '../CapsuleRecording'
import { CapsuleLogo } from '../CapsuleLogo'

vi.mock('../../../lib/tauri', () => ({ abortRecording: vi.fn() }))
vi.mock('../Waveform', () => ({ Waveform: () => null }))
vi.mock('../DurationTimer', () => ({ DurationTimer: () => null }))
vi.mock('../TranslateTargetChip', () => ({ TranslateTargetChip: () => null }))
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }))

describe('capsule mark', () => {
  it('shows the S of the wordmark while recording, hidden from screen readers', () => {
    const { container } = render(<CapsuleRecording />)
    const mark = container.querySelector('.capsule-mark')
    expect(mark?.textContent).toBe('S')
    expect(mark?.getAttribute('aria-hidden')).toBe('true')
  })

  it('draws the idle logo as the five bars of the app icon', () => {
    const { container } = render(<CapsuleLogo />)
    expect(container.querySelectorAll('rect')).toHaveLength(5)
  })
})
