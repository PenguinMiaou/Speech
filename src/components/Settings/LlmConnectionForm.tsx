import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { CheckCircle2, ChevronDown, Globe, Loader2, RefreshCw, XCircle } from 'lucide-react'
import { useAppStore, type LlmApiFormat } from '../../stores/appStore'
import { LLM_DEFAULT_CONFIG, llmProviderRequiresApiKey } from '../../lib/constants'
import {
  CUSTOM_LLM_PROVIDER,
  LLM_API_FORMATS,
  LLM_ENDPOINTS,
  chatModelsOnly,
  llmDefaultApiFormat,
  llmLogoForModel,
  llmProviderFromBaseUrl,
  type LlmEndpoint,
} from '../../lib/llm-endpoints'
import { benchLlmConnection, fetchLlmModels, readCredential, setCredential } from '../../lib/tauri'
import { LlmLogo } from './LlmLogo'
import { FIELD_CLASS, OPTION_CLASS, POPOVER_CLASS, useDismiss } from './shared/popover'

function errorText(error: unknown): string | null {
  if (error instanceof Error) return error.message
  return typeof error === 'string' ? error : null
}

/** Base URL, then API key, then model. The service is worked out from the URL. */
export function LlmConnectionForm() {
  const { t } = useTranslation()
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const testStatus = useAppStore((s) => s.llmTestStatus)
  const setTestStatus = useAppStore((s) => s.setLlmTestStatus)
  const latencyMs = useAppStore((s) => s.llmLatencyMs)
  const setLatencyMs = useAppStore((s) => s.setLlmLatencyMs)
  const models = useAppStore((s) => s.llmModels)
  const setModels = useAppStore((s) => s.setLlmModels)
  const [apiKey, setApiKey] = useState(config.llm_api_key)
  const [fetching, setFetching] = useState(false)
  const [testError, setTestError] = useState<string | null>(null)
  const [reasoningActive, setReasoningActive] = useState(false)
  const [credentialError, setCredentialError] = useState<string | null>(null)
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const fetchTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const provider = config.llm_provider
  const baseUrl = config.llm_base_url
  const apiFormat = config.llm_api_format || llmDefaultApiFormat(provider)
  const requiresApiKey = llmProviderRequiresApiKey(provider)
  const canReachService = Boolean(baseUrl.trim()) && (!requiresApiKey || Boolean(apiKey))

  useEffect(() => {
    let cancelled = false
    const legacyKey = config.llm_api_key
    setApiKey(legacyKey)
    setCredentialError(null)
    readCredential('llm', provider)
      .then((secret) => {
        if (!cancelled) setApiKey(secret ?? legacyKey)
      })
      .catch((error) => console.error('[credentials] failed to read LLM credential', error))
    return () => {
      cancelled = true
    }
  }, [config.llm_api_key, provider])

  const persistKey = useCallback(
    (value: string, delayMs = 350) => {
      if (saveTimer.current) clearTimeout(saveTimer.current)
      saveTimer.current = setTimeout(() => {
        saveTimer.current = null
        setCredential('llm', provider, value)
          .then(() => setCredentialError(null))
          .catch((error) => {
            setCredentialError(errorText(error) ?? String(error))
            console.error('[credentials] failed to save LLM credential', error)
          })
      }, delayMs)
    },
    [provider],
  )

  const refreshModels = useCallback(async () => {
    setFetching(true)
    try {
      setModels(chatModelsOnly(await fetchLlmModels(apiKey, provider, baseUrl, apiFormat)))
    } catch {
      // Keep the current list. Clearing it would make the effect below fetch again at once.
    } finally {
      setFetching(false)
    }
  }, [apiFormat, apiKey, baseUrl, provider, setModels])

  useEffect(() => {
    if (!canReachService || models.length > 0) return
    fetchTimer.current = setTimeout(refreshModels, 500)
    return () => {
      if (fetchTimer.current) clearTimeout(fetchTimer.current)
      fetchTimer.current = null
    }
  }, [canReachService, models.length, refreshModels])

  const resetConnection = () => {
    setTestStatus('idle')
    setLatencyMs(null)
    setTestError(null)
    setReasoningActive(false)
  }

  const typeBaseUrl = (value: string) => {
    const next = llmProviderFromBaseUrl(value)
    updateConfig({
      llm_provider: next,
      llm_base_url: value,
      llm_api_format: next === provider ? apiFormat : llmDefaultApiFormat(next),
    })
    setModels([])
    resetConnection()
  }

  const pickEndpoint = (endpoint: LlmEndpoint) => {
    updateConfig({
      llm_provider: endpoint.provider,
      llm_base_url: endpoint.baseUrl,
      llm_model: endpoint.model,
      llm_api_format: llmDefaultApiFormat(endpoint.provider),
    })
    setModels([])
    resetConnection()
  }

  const chooseFormat = (value: LlmApiFormat) => {
    updateConfig({ llm_api_format: value })
    setModels([])
    resetConnection()
  }

  const handleTest = async () => {
    if (fetchTimer.current) clearTimeout(fetchTimer.current)
    setTestStatus('testing')
    setLatencyMs(null)
    setTestError(null)
    setReasoningActive(false)

    let listed: string[] = []
    let fetchError: string | null = null
    setFetching(true)
    try {
      listed = chatModelsOnly(await fetchLlmModels(apiKey, provider, baseUrl, apiFormat))
      setModels(listed)
    } catch (error) {
      fetchError = errorText(error) ?? t('settings.connectionFailed')
    } finally {
      setFetching(false)
    }

    let model = config.llm_model.trim()
    if (!model) {
      const preferred = LLM_DEFAULT_CONFIG[provider]?.model
      if (preferred && listed.includes(preferred)) {
        model = preferred
        updateConfig({ llm_model: preferred })
      }
    }
    if (!model) {
      if (fetchError || listed.length === 0) {
        setTestError(fetchError ?? t('settings.llmNoModels'))
        setTestStatus('error')
      } else {
        setTestStatus('success')
      }
      return
    }

    // A failed list says little on its own: some services have no model list at all.
    try {
      const result = await benchLlmConnection(apiKey, provider, baseUrl, model, apiFormat)
      setLatencyMs(result.latencyMs)
      setReasoningActive(result.reasoningActive)
      setTestStatus('success')
    } catch (error) {
      setTestError(errorText(error) ?? t('settings.connectionFailed'))
      setTestStatus('error')
    }
  }

  return (
    <div className="space-y-4">
      <div data-field="base-url">
        <FieldLabel>{t('settings.baseUrl')}</FieldLabel>
        <BaseUrlField value={baseUrl} onType={typeBaseUrl} onPick={pickEndpoint} />
      </div>

      <div data-field="api-format">
        <FieldLabel>{t('settings.llmApiFormat')}</FieldLabel>
        <div
          role="radiogroup"
          aria-label={t('settings.llmApiFormat')}
          className="grid grid-cols-3 gap-2"
        >
          {LLM_API_FORMATS.map(({ value, label, short, logo }) => (
            <button
              key={value}
              type="button"
              role="radio"
              aria-checked={value === apiFormat}
              aria-label={label}
              title={label}
              onClick={() => chooseFormat(value)}
              className={`flex items-center gap-2 rounded-[8px] border px-2.5 py-1.5 text-[12px] cursor-pointer ${
                value === apiFormat
                  ? 'border-accent bg-accent/10 text-text-primary'
                  : 'border-border bg-bg-secondary text-text-secondary'
              }`}
            >
              <LlmLogo id={logo} />
              <span className="min-w-0 truncate">{short}</span>
            </button>
          ))}
        </div>
      </div>

      <div data-field="api-key">
        <FieldLabel>{t('settings.apiKey')}</FieldLabel>
        <div className="flex gap-2">
          <input
            type="password"
            aria-label={t('settings.apiKey')}
            value={apiKey}
            onChange={(e) => {
              setApiKey(e.target.value)
              persistKey(e.target.value)
              resetConnection()
              setCredentialError(null)
            }}
            onBlur={() => persistKey(apiKey, 0)}
            placeholder={t(requiresApiKey ? 'settings.enterApiKey' : 'settings.apiKeyOptional')}
            className={`min-w-0 flex-1 px-3 py-2.5 ${FIELD_CLASS}`}
          />
          <button
            type="button"
            onClick={handleTest}
            disabled={!canReachService || testStatus === 'testing'}
            className="flex items-center gap-1.5 rounded-[8px] border-none bg-accent px-4 py-2.5 text-[13px] text-white cursor-pointer disabled:cursor-not-allowed disabled:opacity-40"
          >
            {testStatus === 'testing' && <Loader2 size={14} className="animate-spin" />}
            {t('settings.test')}
          </button>
        </div>
        {testStatus === 'success' && (
          <p className="mt-2 flex items-center gap-1 text-[12px] text-success">
            <CheckCircle2 size={13} />
            {latencyMs !== null ? `${latencyMs}ms` : t('settings.llmPickModel')}
          </p>
        )}
        {reasoningActive && (
          <p role="alert" className="mt-2 text-[12px] text-amber-600">
            {t('settings.llmReasoningActive')}
          </p>
        )}
        {(testStatus === 'error' || testError) && (
          <div className="mt-2 flex items-start gap-1 text-[12px] text-error">
            <XCircle size={13} className="mt-[1px] flex-shrink-0" />
            <span>{testError || t('settings.connectionFailed')}</span>
          </div>
        )}
        <p
          className={`mt-1.5 text-[11px] ${credentialError ? 'text-error' : 'text-text-tertiary'}`}
        >
          {credentialError
            ? t('settings.credentialSaveFailed', { details: credentialError })
            : t('settings.storedLocally')}
        </p>
      </div>

      <div data-field="model">
        <FieldLabel>{t('settings.model')}</FieldLabel>
        <div className="flex gap-2">
          <ModelPicker
            value={config.llm_model}
            models={models}
            onChange={(model) => {
              updateConfig({ llm_model: model })
              setLatencyMs(null)
              setTestError(null)
            }}
          />
          <button
            type="button"
            onClick={refreshModels}
            disabled={fetching || !canReachService}
            title={t('settings.fetchModels')}
            className="rounded-[8px] border border-border bg-bg-secondary px-3 py-2.5 text-text-secondary cursor-pointer disabled:cursor-not-allowed disabled:opacity-40"
          >
            <RefreshCw size={14} className={fetching ? 'animate-spin' : ''} />
          </button>
        </div>
        {models.length > 0 && (
          <p className="mt-1 text-[11px] text-text-tertiary">
            {t('settings.modelsAvailable', { count: models.length })}
          </p>
        )}
      </div>

      <div data-field="fallback-model">
        <label>
          <FieldLabel>{t('settings.fallbackModel')}</FieldLabel>
          <input
            type="text"
            value={config.llm_fallback_model}
            onChange={(event) => updateConfig({ llm_fallback_model: event.target.value })}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            className={`w-full px-2.5 py-2 ${FIELD_CLASS}`}
          />
        </label>
        <p className="mt-1 text-[11px] text-text-tertiary">{t('settings.fallbackModelHint')}</p>
      </div>
    </div>
  )
}

function FieldLabel({ children }: { children: React.ReactNode }) {
  return <p className="mb-1.5 text-[13px] font-medium text-text-secondary">{children}</p>
}

function BaseUrlField({
  value,
  onType,
  onPick,
}: {
  value: string
  onType: (value: string) => void
  onPick: (endpoint: LlmEndpoint) => void
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  // The list narrows only to what is being typed; opened any other way it shows every service,
  // or a URL that matches none of them would open an empty list.
  const [typing, setTyping] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)
  const close = useCallback(() => setOpen(false), [])
  useDismiss(rootRef, open, close)
  const showAll = (next: boolean) => {
    setTyping(false)
    setOpen(next)
  }

  const provider = llmProviderFromBaseUrl(value)
  const service = LLM_ENDPOINTS.find((endpoint) => endpoint.provider === provider)
  const query = value.trim().toLowerCase()
  const isListed = LLM_ENDPOINTS.some((endpoint) => endpoint.baseUrl === value.trim())
  const shown =
    !typing || !query || isListed
      ? LLM_ENDPOINTS
      : LLM_ENDPOINTS.filter(
          (endpoint) =>
            endpoint.baseUrl.includes(query) ||
            endpoint.provider.includes(query) ||
            t(endpoint.labelKey).toLowerCase().includes(query),
        )

  return (
    <div ref={rootRef} className="relative">
      <div className="absolute left-2.5 top-1/2 -translate-y-1/2">
        <LlmLogo id={provider === CUSTOM_LLM_PROVIDER ? null : provider} fallback={Globe} />
      </div>
      <input
        aria-label={t('settings.baseUrl')}
        value={value}
        onFocus={() => showAll(true)}
        onChange={(e) => {
          onType(e.target.value)
          setTyping(true)
          setOpen(true)
        }}
        onKeyDown={(e) => {
          if (e.key === 'Escape') setOpen(false)
        }}
        placeholder="https://api.openai.com/v1"
        className={`w-full py-2.5 pl-10 pr-9 ${FIELD_CLASS}`}
      />
      <button
        type="button"
        aria-label={t('settings.llmChooseService')}
        aria-expanded={open}
        title={service ? t(service.labelKey) : undefined}
        onClick={() => showAll(!open)}
        className="absolute right-1 top-1 flex h-[calc(100%-8px)] w-8 cursor-pointer items-center justify-center rounded-[8px] border-none bg-transparent text-text-tertiary hover:bg-bg-tertiary hover:text-text-primary"
      >
        <ChevronDown size={14} />
      </button>
      {open && shown.length > 0 && (
        <div role="listbox" className={POPOVER_CLASS}>
          {shown.map((endpoint) => (
            <button
              key={endpoint.provider}
              type="button"
              role="option"
              aria-selected={endpoint.baseUrl === value.trim()}
              onClick={() => {
                onPick(endpoint)
                setOpen(false)
              }}
              className={OPTION_CLASS}
            >
              <LlmLogo id={endpoint.provider} />
              <span className="flex-shrink-0">{t(endpoint.labelKey)}</span>
              <span className="min-w-0 truncate text-[11px] text-text-tertiary">
                {endpoint.baseUrl}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

function ModelPicker({
  value,
  models,
  onChange,
}: {
  value: string
  models: string[]
  onChange: (model: string) => void
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const rootRef = useRef<HTMLDivElement>(null)
  const close = useCallback(() => {
    setOpen(false)
    setQuery('')
    setActive(0)
  }, [])
  useDismiss(rootRef, open, close)

  const typed = query.trim()
  const shown = typed
    ? models.filter((model) => model.toLowerCase().includes(typed.toLowerCase()))
    : models

  const commit = (model: string) => {
    onChange(model)
    close()
  }

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Escape') {
      close()
    } else if (event.key === 'ArrowDown') {
      event.preventDefault()
      setActive((index) => Math.min(index + 1, shown.length - 1))
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      setActive((index) => Math.max(index - 1, 0))
    } else if (event.key === 'Enter') {
      event.preventDefault()
      const choice = shown[active] ?? typed
      if (choice) commit(choice)
    }
  }

  return (
    <div ref={rootRef} className="relative min-w-0 flex-1">
      <button
        type="button"
        aria-label={t('settings.model')}
        aria-expanded={open}
        onClick={() => (open ? close() : setOpen(true))}
        className={`flex w-full items-center gap-2.5 px-2.5 py-2 cursor-pointer ${FIELD_CLASS}`}
      >
        <LlmLogo id={value ? llmLogoForModel(value) : null} />
        <span className={`min-w-0 flex-1 truncate text-left ${value ? '' : 'text-text-tertiary'}`}>
          {value || t('settings.llmChooseModel')}
        </span>
        <ChevronDown size={14} className="flex-shrink-0 text-text-tertiary" />
      </button>
      {open && (
        <div className={POPOVER_CLASS}>
          <input
            autoFocus
            value={query}
            onChange={(e) => {
              setQuery(e.target.value)
              setActive(0)
            }}
            onKeyDown={handleKeyDown}
            placeholder={t('settings.modelSearchPlaceholder')}
            className={`mb-1 w-full px-2.5 py-1.5 ${FIELD_CLASS}`}
          />
          {shown.length > 0 ? (
            <div role="listbox">
              {shown.map((model, index) => (
                <button
                  key={model}
                  type="button"
                  role="option"
                  aria-selected={model === value}
                  onClick={() => commit(model)}
                  onMouseEnter={() => setActive(index)}
                  className={`${OPTION_CLASS} ${index === active ? 'bg-bg-secondary' : ''}`}
                >
                  <LlmLogo id={llmLogoForModel(model)} />
                  <span className="min-w-0 truncate">{model}</span>
                </button>
              ))}
            </div>
          ) : (
            <p className="px-2 py-1.5 text-[12px] text-text-tertiary">
              {t(typed ? 'settings.modelUseTyped' : 'settings.modelListEmpty')}
            </p>
          )}
        </div>
      )}
    </div>
  )
}
