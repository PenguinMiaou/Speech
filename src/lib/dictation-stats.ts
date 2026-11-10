/** A person types about this many words, or Chinese characters, a minute. */
const TYPED_PER_MINUTE = 40

interface Dictated {
  polished_text: string
  duration_ms: number | null
}

/** Chinese counts by character, everything else by word. */
function units(text: string): number {
  const chinese = text.match(/[\u4e00-\u9fff]/g)?.length ?? 0
  const words = text.replace(/[\u4e00-\u9fff]/g, ' ').match(/[\p{L}\p{N}]+/gu)?.length ?? 0
  return chinese + words
}

/** How much was dictated, and the minutes it saved over typing the same text. */
export function dictationStats(history: Dictated[]): { units: number; savedMinutes: number } {
  let total = 0
  let saved = 0
  for (const entry of history) {
    const count = units(entry.polished_text)
    total += count
    if (entry.duration_ms !== null) {
      saved += Math.max(0, count / TYPED_PER_MINUTE - entry.duration_ms / 60_000)
    }
  }
  return { units: total, savedMinutes: Math.round(saved) }
}

export function formatMinutes(minutes: number): string {
  return minutes < 60 ? `${minutes} min` : `${Number((minutes / 60).toFixed(1))} h`
}
