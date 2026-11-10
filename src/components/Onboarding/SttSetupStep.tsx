import { useTranslation } from 'react-i18next'
import { CheckCircle2, XCircle, Loader2 } from 'lucide-react'
import { useAppStore, type AppConfig } from '../../stores/appStore'
import {
  APPLE_SPEECH_PROVIDER,
  CUSTOM_WHISPER_PROVIDER,
  LOCAL_QWEN3_PROVIDER,
  ONBOARDING_STT_PROVIDERS,
  STT_PROVIDERS,
} from '../../lib/constants'
import { testSttConnection } from '../../lib/tauri'
import { LocalModelCard } from '../Settings/LocalModelCard'
import { SttProviderSelect } from '../Settings/SttProviderSelect'

export function SttSetupStep() {
  const { t } = useTranslation()
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const setSttTestStatus = useAppStore((s) => s.setSttTestStatus)
  const isLocal = config.stt_provider === LOCAL_QWEN3_PROVIDER

  const select = (provider: string) => {
    updateConfig({ stt_provider: provider as AppConfig['stt_provider'] })
    setSttTestStatus('idle')
  }

  return (
    <div className="space-y-5">
      <div role="tablist" className="flex gap-1 rounded-[8px] bg-bg-secondary p-1">
        <ModeTab
          selected={isLocal}
          label={t('onboarding.stt.localTab')}
          onSelect={() => select(LOCAL_QWEN3_PROVIDER)}
        />
        <ModeTab
          selected={!isLocal}
          label={t('onboarding.stt.apiTab')}
          onSelect={() => select(ONBOARDING_STT_PROVIDERS[0].value)}
        />
      </div>

      {isLocal ? (
        <div className="space-y-2">
          <LocalModelCard />
          <p className="text-[12px] text-text-tertiary">{t('onboarding.stt.localHint')}</p>
        </div>
      ) : (
        <ApiForm onSelect={select} />
      )}
    </div>
  )
}

function ModeTab({
  selected,
  label,
  onSelect,
}: {
  selected: boolean
  label: string
  onSelect: () => void
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={selected}
      onClick={() => {
        if (!selected) onSelect()
      }}
      className={`flex-1 rounded-[8px] border-none px-3 py-2 text-[13px] font-medium cursor-pointer ${
        selected ? 'bg-accent text-white' : 'bg-transparent text-text-secondary'
      }`}
    >
      {label}
    </button>
  )
}

function ApiForm({ onSelect }: { onSelect: (provider: string) => void }) {
  const { t } = useTranslation()
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const sttTestStatus = useAppStore((s) => s.sttTestStatus)
  const setSttTestStatus = useAppStore((s) => s.setSttTestStatus)
  const isCustomWhisper = config.stt_provider === CUSTOM_WHISPER_PROVIDER
  const isAppleSpeech = config.stt_provider === APPLE_SPEECH_PROVIDER
  const options = isAppleSpeech
    ? STT_PROVIDERS.filter(
        (provider) =>
          provider.value === APPLE_SPEECH_PROVIDER ||
          ONBOARDING_STT_PROVIDERS.some((listed) => listed.value === provider.value),
      )
    : ONBOARDING_STT_PROVIDERS

  const handleTest = async () => {
    setSttTestStatus('testing')
    try {
      const ok = isCustomWhisper
        ? await testSttConnection(
            config.stt_custom_api_key,
            CUSTOM_WHISPER_PROVIDER,
            config.stt_custom_base_url,
            config.stt_custom_model,
          )
        : await testSttConnection(config.stt_api_key, config.stt_provider)
      setSttTestStatus(ok ? 'success' : 'error')
    } catch {
      setSttTestStatus('error')
    }
  }

  return (
    <>
      {isCustomWhisper ? (
        <Field label={t('onboarding.stt.serviceLabel')}>
          <div className="rounded-[8px] border border-border bg-bg-secondary px-3 py-2.5">
            <p className="text-[13px] font-medium text-text-primary">
              {t('onboarding.stt.customWhisperConfigured')}
            </p>
            <p className="mt-1 truncate text-[11px] text-text-tertiary">
              {config.stt_custom_base_url}
            </p>
            <p className="mt-0.5 truncate text-[11px] text-text-tertiary">
              {config.stt_custom_model}
            </p>
          </div>
        </Field>
      ) : (
        <Field label={t('onboarding.stt.serviceLabel')}>
          <SttProviderSelect
            label={t('onboarding.stt.serviceLabel')}
            value={config.stt_provider}
            providers={options}
            onChange={onSelect}
          />
        </Field>
      )}

      {!isAppleSpeech && (
        <Field label={t('onboarding.stt.apiKeyLabel')}>
          <div className="flex gap-2">
            <input
              type="password"
              value={isCustomWhisper ? config.stt_custom_api_key : config.stt_api_key}
              onChange={(e) => {
                updateConfig(
                  isCustomWhisper
                    ? { stt_custom_api_key: e.target.value }
                    : { stt_api_key: e.target.value },
                )
                setSttTestStatus('idle')
              }}
              placeholder={t('onboarding.stt.apiKeyPlaceholder')}
              className="flex-1 px-3 py-2.5 bg-bg-secondary border border-border rounded-[8px] text-[13px] text-text-primary placeholder:text-text-tertiary outline-none focus:border-accent"
            />
            <button
              onClick={handleTest}
              disabled={
                sttTestStatus === 'testing' ||
                (isCustomWhisper
                  ? !config.stt_custom_base_url.trim() || !config.stt_custom_model.trim()
                  : !config.stt_api_key)
              }
              className="px-4 py-2.5 bg-accent text-white rounded-[8px] text-[13px] border-none cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed flex items-center gap-1.5"
            >
              {sttTestStatus === 'testing' && <Loader2 size={14} className="animate-spin" />}
              {t('onboarding.stt.testButton')}
            </button>
          </div>
          <TestStatusHint status={sttTestStatus} />
        </Field>
      )}
    </>
  )
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <p className="mb-2 text-[13px] font-medium text-text-secondary">{label}</p>
      {children}
    </div>
  )
}

function TestStatusHint({ status }: { status: string }) {
  const { t } = useTranslation()
  if (status === 'success') {
    return (
      <p className="flex items-center gap-1 text-[12px] text-success mt-2">
        <CheckCircle2 size={13} /> {t('onboarding.stt.connectionOk')}
      </p>
    )
  }
  if (status === 'error') {
    return (
      <p className="flex items-center gap-1 text-[12px] text-error mt-2">
        <XCircle size={13} /> {t('onboarding.stt.connectionFail')}
      </p>
    )
  }
  return null
}
