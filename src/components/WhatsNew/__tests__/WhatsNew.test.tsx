import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { version } from '../../../../package.json'
import { WHATS_NEW } from '../../../lib/whatsNew'
import { WhatsNew, markWhatsNewSeen } from '..'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, string>) =>
      values?.version ? `${key} ${values.version}` : key,
    i18n: { language: 'zh' },
  }),
}))

describe('WhatsNew', () => {
  beforeEach(() => localStorage.clear())

  it('tells what this version changed after an update, until it is closed', () => {
    localStorage.setItem('whats_new_seen', '0.0.1')
    render(<WhatsNew />)

    expect(screen.getByRole('dialog')).toBeDefined()
    expect(screen.getByText(`whatsNew.title ${version}`)).toBeDefined()
    expect(screen.getByText(WHATS_NEW[version].zh[0])).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: 'common.close' }))
    expect(screen.queryByRole('dialog')).toBeNull()
    expect(localStorage.getItem('whats_new_seen')).toBe(version)
  })

  it('stays away once this version was shown', () => {
    markWhatsNewSeen()
    render(<WhatsNew />)
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('does not count as seen until it is closed', () => {
    localStorage.setItem('whats_new_seen', '0.0.1')
    const view = render(<WhatsNew />)
    view.unmount()
    expect(localStorage.getItem('whats_new_seen')).toBe('0.0.1')
  })
})
