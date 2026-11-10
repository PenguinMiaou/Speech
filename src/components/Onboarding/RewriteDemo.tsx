import { Mic } from 'lucide-react'
import { useCallback, useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { updateConfig as persistConfig, voiceIsolationEnrollNext } from '../../lib/tauri'
import { useAppStore } from '../../stores/appStore'
import { Waveform } from '../Capsule/Waveform'
import { Toggle } from '../Settings/shared/Toggle'

/** A selected sentence that a spoken instruction rewrites in place. */
export function RewriteDemo() {
  const { t } = useTranslation()
  const hotkey = useAppStore((s) => s.config.hotkey)
  const allowed = useAppStore((s) => s.config.selected_text_enabled)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const pipelineState = useAppStore((s) => s.pipelineState)
  const said = useAppStore((s) => (s.finalTranscript || s.partialTranscript).trim())
  const resetRecording = useAppStore((s) => s.resetRecording)
  const draft = useRef<HTMLTextAreaElement>(null)
  const previousState = useRef(pipelineState)
  const text = t('onboarding.tryIt.rewriteDraft')
  const idle = pipelineState === 'idle'
  const listening = pipelineState === 'recording'

  // The pipeline rewrites whatever is selected in the field that has focus.
  const selectDraft = useCallback(() => {
    const field = draft.current
    if (!field) return
    field.value = text
    field.focus()
    field.select()
  }, [text])

  useEffect(selectDraft, [selectDraft])

  // A few spoken words are too short to be a voice sample.
  useEffect(() => {
    voiceIsolationEnrollNext(false)?.catch(() => {})
  }, [])

  useEffect(() => {
    const wasIdle = previousState.current === 'idle'
    previousState.current = pipelineState
    if (wasIdle && pipelineState !== 'idle') resetRecording()
  }, [pipelineState, resetRecording])

  const allow = (selected_text_enabled: boolean) => {
    updateConfig({ selected_text_enabled })
    persistConfig({ ...useAppStore.getState().config, selected_text_enabled })?.catch(() => {})
    selectDraft()
  }

  const again = () => {
    resetRecording()
    selectDraft()
  }

  return (
    <div className="space-y-4">
      <textarea
        ref={draft}
        aria-label={t('onboarding.tryIt.demos.rewrite')}
        defaultValue={text}
        rows={3}
        spellCheck={false}
        className="w-full resize-none rounded-xl border border-border bg-bg-secondary px-5 py-4 text-[15px] leading-relaxed text-text-primary outline-none focus:border-border-focus"
      />

      {allowed ? (
        <div className="flex min-h-[36px] items-center justify-center gap-3 text-[12px] text-text-secondary">
          {idle ? (
            <span className="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full bg-accent text-white">
              <Mic size={14} />
            </span>
          ) : (
            <Waveform bars={28} maxHeight={28} barClassName="bg-accent" idle={!listening} />
          )}
          <span>
            {idle ? t('onboarding.tryIt.rewriteHint', { hotkey }) : t('onboarding.tryIt.working')}
          </span>
        </div>
      ) : (
        <div className="flex justify-center">
          <Toggle checked={false} onChange={allow} label={t('onboarding.tryIt.rewriteSwitch')} />
        </div>
      )}

      {idle && said && (
        <div className="flex items-center justify-center gap-3 text-[12px] text-text-tertiary">
          <span>
            <span className="mr-2 font-medium text-text-secondary">
              {t('onboarding.tryIt.raw')}
            </span>
            {said}
          </span>
          <button
            type="button"
            onClick={again}
            className="rounded-[10px] border border-border bg-bg-secondary px-3 py-1.5 text-text-primary transition-colors hover:bg-bg-tertiary"
          >
            {t('onboarding.tryIt.again')}
          </button>
        </div>
      )}
    </div>
  )
}
