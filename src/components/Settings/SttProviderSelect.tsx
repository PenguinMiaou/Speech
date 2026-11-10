import { Cpu, Server, type LucideIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { CUSTOM_WHISPER_PROVIDER, LOCAL_QWEN3_PROVIDER } from '../../lib/constants'
import { LogoSelect } from './shared/LogoSelect'

const FALLBACK_ICONS: Record<string, LucideIcon> = {
  [LOCAL_QWEN3_PROVIDER]: Cpu,
  [CUSTOM_WHISPER_PROVIDER]: Server,
}

interface Props {
  label: string
  value: string
  providers: readonly { value: string; labelKey: string; logo: string | null }[]
  onChange: (provider: string) => void
}

export function SttProviderSelect({ label, value, providers, onChange }: Props) {
  const { t } = useTranslation()
  return (
    <LogoSelect
      label={label}
      value={value}
      onChange={onChange}
      options={providers.map((provider) => ({
        value: provider.value,
        label: t(provider.labelKey),
        logo: provider.logo,
        fallback: FALLBACK_ICONS[provider.value],
      }))}
    />
  )
}
