import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { AudioLines, CheckCircle2, ChevronDown, Cpu, Loader2, XCircle } from 'lucide-react'
import { useAppStore } from '../../stores/appStore'
import { LOCAL_QWEN3_PROVIDER } from '../../lib/constants'
import { LlmLogo } from './LlmLogo'
import {
  cancelLocalInstall,
  deleteLocalModel,
  installLocalModel,
  listLocalModels,
  onLocalSttProgress,
  type LocalModelStatus,
  type LocalSttProgress,
} from '../../lib/tauri'
import { formatBytes } from '../../lib/format-bytes'

const MODELS: Record<string, { name: string; logo: string | null; note: string; main?: boolean }> =
  {
    'qwen3-asr-1.7b-q8': {
      name: 'Qwen3 1.7B',
      logo: 'qwen',
      note: 'localStt.notes.qwenLarge',
      main: true,
    },
    'qwen3-asr-0.6b-q8': {
      name: 'Qwen3 0.6B',
      logo: 'qwen',
      note: 'localStt.notes.qwenSmall',
      main: true,
    },
    'qwen3-asr-1.7b-f16': {
      name: 'Qwen3 1.7B F16',
      logo: 'qwen',
      note: 'localStt.notes.qwenLargeFull',
    },
    'moss-transcribe-diarize-q8': {
      name: 'MOSS Transcribe',
      logo: null,
      note: 'localStt.notes.moss',
    },
    'cohere-transcribe-q8': {
      name: 'Cohere Transcribe',
      logo: 'cohere',
      note: 'localStt.notes.cohere',
    },
    'voxtral-mini-4b-q8': {
      name: 'Voxtral Mini 4B',
      logo: 'mistral',
      note: 'localStt.notes.voxtral',
    },
    'nemotron-3.5-0.6b-q8': {
      name: 'Nemotron 0.6B',
      logo: 'nvidia',
      note: 'localStt.notes.nemotron',
    },
  }

const ERROR_KEYS: Record<string, string> = {
  disk_full: 'localStt.errorDiskFull',
  network: 'localStt.errorNetwork',
  checksum: 'localStt.errorChecksum',
  engine: 'localStt.errorEngine',
  engine_cpu: 'localStt.errorEngineCpu',
  engine_slow: 'localStt.errorEngineSlow',
  engine_blocked: 'localStt.errorEngineBlocked',
}

const BUTTON_PRIMARY =
  'rounded-[8px] border border-accent bg-accent px-3 py-1.5 text-[12px] font-medium text-white hover:bg-accent-hover cursor-pointer'
const BUTTON_QUIET =
  'rounded-[8px] border border-border bg-transparent px-3 py-1.5 text-[12px] text-text-secondary hover:text-text-primary cursor-pointer'

type RowState =
  | 'missing'
  | 'partial'
  | 'failed'
  | 'downloading'
  | 'verifying'
  | 'testing'
  | 'installed'
  | 'active'

function rowState(
  model: LocalModelStatus,
  progress: LocalSttProgress | undefined,
  selected: boolean,
): RowState {
  const phase = progress?.phase
  if (phase === 'downloading' || phase === 'verifying' || phase === 'testing') return phase
  if (model.installing) return model.installed ? 'testing' : 'downloading'
  if (phase === 'failed') return 'failed'
  if (model.installed) return selected ? 'active' : 'installed'
  return model.partial_bytes > 0 ? 'partial' : 'missing'
}

export function LocalModelCard() {
  const config = useAppStore((s) => s.config)
  const { t } = useTranslation()
  const [models, setModels] = useState<LocalModelStatus[]>([])
  const [loaded, setLoaded] = useState(false)
  const [progress, setProgress] = useState<Record<string, LocalSttProgress>>({})
  const [confirmingDelete, setConfirmingDelete] = useState<string | null>(null)
  const [showAll, setShowAll] = useState(false)

  const refresh = useCallback(async () => {
    const list = (await Promise.resolve(listLocalModels()).catch(() => null)) ?? []
    setModels(list)
    setLoaded(true)
  }, [])

  useEffect(() => {
    void refresh()
    const unlisten = Promise.resolve(
      onLocalSttProgress((event) => {
        setProgress((current) => ({ ...current, [event.model_id]: event }))
        if (event.phase === 'ready' || event.phase === 'failed' || event.phase === 'cancelled') {
          void refresh()
        }
      }),
    )
    return () => {
      void unlisten.then((stop) => stop?.())
    }
  }, [refresh])

  const install = (modelId: string) => {
    setProgress((current) => {
      const { [modelId]: _previous, ...rest } = current
      return rest
    })
    setModels((current) => current.map((m) => (m.id === modelId ? { ...m, installing: true } : m)))
    installLocalModel(modelId).catch(() => {
      setProgress((current) => ({
        ...current,
        [modelId]: {
          model_id: modelId,
          phase: 'failed',
          downloaded: 0,
          total: 0,
          bytes_per_sec: 0,
          error_code: null,
        },
      }))
      void refresh()
    })
  }

  const remove = async (modelId: string) => {
    setConfirmingDelete(null)
    await deleteLocalModel(modelId)
    await refresh()
  }

  const usingLocal = config.stt_provider === LOCAL_QWEN3_PROVIDER
  const selectedMissing =
    loaded && usingLocal && !models.some((m) => m.id === config.stt_local_model && m.installed)

  const shown = models.filter(
    (model) =>
      showAll ||
      MODELS[model.id]?.main ||
      model.installed ||
      model.installing ||
      model.partial_bytes > 0 ||
      progress[model.id] !== undefined,
  )
  const hidden = models.length - shown.length

  return (
    <div className="rounded-[12px] border border-border p-4 space-y-3">
      <div className="flex items-center gap-2 text-[13px] font-medium text-text-primary">
        <Cpu size={15} />
        {t('localStt.title')}
      </div>

      {selectedMissing && (
        <p className="flex items-center gap-1 text-[12px] text-error">
          <XCircle size={13} className="flex-shrink-0" />
          <span>{t('localStt.notInstalled')}</span>
        </p>
      )}

      <div className="divide-y divide-border">
        {shown.map((model) => {
          const event = progress[model.id]
          const state = rowState(model, event, usingLocal && config.stt_local_model === model.id)
          const info = MODELS[model.id]
          const downloaded = event?.downloaded ?? model.partial_bytes
          const percent = Math.floor((downloaded / model.size_bytes) * 100)
          const errorKey = event?.error_code ? ERROR_KEYS[event.error_code] : undefined
          return (
            <div key={model.id} className="space-y-1.5 py-3 first:pt-0 last:pb-0">
              <div className="flex items-center gap-3">
                <LlmLogo id={info?.logo ?? null} fallback={AudioLines} />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-x-2">
                    <span className="text-[13px] font-medium text-text-primary">
                      {info?.name ?? model.id}
                    </span>
                    {model.recommended && (
                      <span className="rounded-full bg-accent/10 px-1.5 text-[11px] leading-[18px] text-accent">
                        {t('localStt.recommended')}
                      </span>
                    )}
                    <span className="text-[12px] text-text-tertiary">
                      {formatBytes(model.size_bytes)}
                    </span>
                  </div>
                  {info && (
                    <p className="mt-0.5 text-[12px] leading-snug text-text-tertiary">
                      {t(info.note)}
                    </p>
                  )}
                </div>
                <div className="flex flex-shrink-0 items-center gap-2">
                  {state === 'missing' && (
                    <button
                      type="button"
                      className={BUTTON_PRIMARY}
                      onClick={() => install(model.id)}
                    >
                      {t('localStt.install')}
                    </button>
                  )}
                  {state === 'partial' && (
                    <button
                      type="button"
                      className={BUTTON_PRIMARY}
                      onClick={() => install(model.id)}
                    >
                      {t('localStt.resume')}
                    </button>
                  )}
                  {state === 'failed' && (
                    <button
                      type="button"
                      className={BUTTON_PRIMARY}
                      onClick={() => install(model.id)}
                    >
                      {t('localStt.retry')}
                    </button>
                  )}
                  {state === 'downloading' && (
                    <button
                      type="button"
                      className={BUTTON_QUIET}
                      onClick={() => cancelLocalInstall(model.id)}
                    >
                      {t('localStt.cancel')}
                    </button>
                  )}
                  {(state === 'verifying' || state === 'testing') && (
                    <span className="flex items-center gap-1 text-[12px] text-text-secondary">
                      <Loader2 size={13} className="animate-spin" />
                      {t(state === 'verifying' ? 'localStt.verifying' : 'localStt.testing')}
                    </span>
                  )}
                  {state === 'installed' && (
                    <button
                      type="button"
                      className={BUTTON_PRIMARY}
                      onClick={() => install(model.id)}
                    >
                      {t('localStt.enable')}
                    </button>
                  )}
                  {state === 'active' && (
                    <span className="flex items-center gap-1 text-[12px] text-success">
                      <CheckCircle2 size={13} />
                      {t('localStt.active')}
                    </span>
                  )}
                  {(state === 'installed' ||
                    state === 'active' ||
                    (state === 'failed' && model.installed)) &&
                    (confirmingDelete === model.id ? (
                      <button
                        type="button"
                        className={BUTTON_QUIET}
                        onClick={() => void remove(model.id)}
                      >
                        {t('localStt.confirmDelete')}
                      </button>
                    ) : (
                      <button
                        type="button"
                        className={BUTTON_QUIET}
                        onClick={() => setConfirmingDelete(model.id)}
                      >
                        {t('localStt.delete')}
                      </button>
                    ))}
                </div>
              </div>

              {state === 'downloading' && (
                <div className="space-y-1">
                  <div
                    role="progressbar"
                    aria-valuemin={0}
                    aria-valuemax={100}
                    aria-valuenow={percent}
                    className="h-1 overflow-hidden rounded-full bg-border"
                  >
                    <div
                      className="h-full bg-accent transition-all"
                      style={{ width: `${percent}%` }}
                    />
                  </div>
                  <p className="text-[12px] text-text-tertiary">
                    {formatBytes(downloaded)} / {formatBytes(model.size_bytes)}
                    {event?.bytes_per_sec ? ` (${formatBytes(event.bytes_per_sec)}/s)` : ''}
                  </p>
                </div>
              )}

              {state === 'failed' && (
                <div className="text-[12px]">
                  <p className="flex items-center gap-1 text-error">
                    <XCircle size={13} className="flex-shrink-0" />
                    <span>{t(errorKey ?? 'localStt.errorUnknown')}</span>
                  </p>
                  {event?.error_detail && (
                    <p className="mt-1 ml-[17px] select-text break-all font-mono text-[11px] text-text-tertiary">
                      {event.error_detail}
                    </p>
                  )}
                </div>
              )}
            </div>
          )
        })}
      </div>

      {hidden > 0 && (
        <button
          type="button"
          onClick={() => setShowAll(true)}
          className="flex w-full cursor-pointer items-center justify-center gap-1 rounded-[8px] border-none bg-transparent py-1 text-[12px] text-text-secondary hover:text-text-primary"
        >
          {t('localStt.moreModels')} ({hidden})
          <ChevronDown size={13} />
        </button>
      )}
    </div>
  )
}
