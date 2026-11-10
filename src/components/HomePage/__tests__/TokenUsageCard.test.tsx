import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { TokenUsageCard, formatTokens } from '../TokenUsageCard'
import * as tauri from '../../../lib/tauri'

vi.mock('../../../lib/tauri')
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

const span = (input: number, output: number, requests: number) => ({ input, output, requests })

describe('TokenUsageCard', () => {
  afterEach(cleanup)

  it('shows tokens used today, in the last 30 days and in all', async () => {
    vi.mocked(tauri.tokenUsageSummary).mockResolvedValue({
      today: span(900, 100, 4),
      last30Days: span(40_000, 5_600, 120),
      total: span(1_900_000, 300_000, 900),
    })
    render(<TokenUsageCard />)

    expect(await screen.findByText('1,000')).toBeTruthy()
    expect(screen.getByText('45.6k')).toBeTruthy()
    expect(screen.getByText('2.2M')).toBeTruthy()
  })

  it('stays hidden while nothing has been reported', async () => {
    vi.mocked(tauri.tokenUsageSummary).mockResolvedValue({
      today: span(0, 0, 0),
      last30Days: span(0, 0, 0),
      total: span(0, 0, 0),
    })
    const { container } = render(<TokenUsageCard />)
    await Promise.resolve()
    expect(container.textContent).toBe('')
  })

  it('writes large counts compactly', () => {
    expect(formatTokens(999)).toBe('999')
    expect(formatTokens(9_999)).toBe('9,999')
    expect(formatTokens(10_000)).toBe('10k')
    expect(formatTokens(1_250_000)).toBe('1.3M')
  })
})
