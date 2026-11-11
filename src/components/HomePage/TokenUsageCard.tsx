import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { tokenUsageSummary, type TokenUsageSummary, type TokenUsageSpan } from '../../lib/tauri'

/** 1,234 up to ten thousand, then 45.6k and 2.2M. */
export function formatTokens(count: number): string {
  const short = (value: number, unit: string) => `${Number(value.toFixed(1))}${unit}`
  if (count >= 1_000_000) return short(count / 1_000_000, 'M')
  if (count >= 10_000) return short(count / 1_000, 'k')
  return count.toLocaleString('en-US')
}

const used = (span: TokenUsageSpan) => span.input + span.output

/** Tokens the polish model reported using. Hidden until a provider reports any. */
export function TokenUsageCard() {
  const { t } = useTranslation()
  const [summary, setSummary] = useState<TokenUsageSummary | null>(null)

  useEffect(() => {
    let alive = true
    tokenUsageSummary()
      ?.then((value) => {
        if (alive) setSummary(value)
      })
      .catch(() => {})
    return () => {
      alive = false
    }
  }, [])

  if (!summary || used(summary.total) === 0) return null

  const spans = [
    ['home.tokens.today', summary.today],
    ['home.tokens.last30Days', summary.last30Days],
    ['home.tokens.total', summary.total],
  ] as const

  return (
    <div className="border-b border-border pb-4">
      <h3 className="mb-3 text-[13px] font-medium text-text-secondary">{t('home.tokens.title')}</h3>
      <div className="grid grid-cols-3 gap-3">
        {spans.map(([label, span]) => (
          <div key={label}>
            <p className="mb-1 text-[12px] text-text-tertiary">{t(label)}</p>
            <p
              className="text-[18px] font-semibold tabular-nums"
              title={`${span.input.toLocaleString('en-US')} + ${span.output.toLocaleString('en-US')}`}
            >
              {formatTokens(used(span))}
            </p>
          </div>
        ))}
      </div>
    </div>
  )
}
