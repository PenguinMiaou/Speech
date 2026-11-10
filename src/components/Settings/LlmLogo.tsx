import type { LucideIcon } from 'lucide-react'
import { Box } from 'lucide-react'

const LOGO_URLS = import.meta.glob('../../assets/llm-logos/*.svg', {
  eager: true,
  query: '?url',
  import: 'default',
}) as Record<string, string>

interface Props {
  id: string | null
  fallback?: LucideIcon
}

/** Some marks are single-color black, so every logo sits on a white tile in both themes. */
export function LlmLogo({ id, fallback: Fallback = Box }: Props) {
  const src = id ? LOGO_URLS[`../../assets/llm-logos/${id}.svg`] : undefined
  return (
    <span className="inline-flex h-[22px] w-[22px] flex-shrink-0 items-center justify-center rounded-[6px] border border-border bg-white">
      {src ? (
        <img src={src} alt="" className="h-[15px] w-[15px]" />
      ) : (
        <Fallback size={13} className="text-neutral-500" />
      )}
    </span>
  )
}
