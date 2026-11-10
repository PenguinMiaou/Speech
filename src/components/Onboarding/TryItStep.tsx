import { motion, useReducedMotion } from 'framer-motion'
import { Mic } from 'lucide-react'
import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react'
import { useTranslation } from 'react-i18next'
import { spring } from '../../lib/animations'
import { LOCAL_QWEN3_PROVIDER } from '../../lib/constants'
import { voiceIsolationEnrollNext, voiceIsolationStatus } from '../../lib/tauri'
import { diffText, type DiffPart } from '../../lib/text-diff'
import { useAppStore } from '../../stores/appStore'
import { Waveform } from '../Capsule/Waveform'

type Stage = 'raw' | 'strike' | 'collapse' | 'final'

const STRIKE_START_MS = 450
const STRIKE_MS = 260
const LONGEST_STAGGER_MS = 130
const ALL_STAGGERS_MS = 1100
const HOLD_STRUCK_MS = 450
const COLLAPSE_MS = 520
const LIST_MARK = /^(\d+[.、)）]|[-•])(\s*)(.*)$/

const SURFACE =
  'whitespace-pre-wrap break-words rounded-xl border border-border bg-bg-secondary px-5 py-4 text-[15px] leading-relaxed text-text-primary'

function strikeStagger(parts: DiffPart[]): number {
  const removed = parts.filter((part) => part.kind === 'removed').length
  return removed > 0 ? Math.min(LONGEST_STAGGER_MS, ALL_STAGGERS_MS / removed) : 0
}

/** Walks a finished result through the cleanup animation, one stage at a time. */
function useRevealStage(parts: DiffPart[] | null, reduced: boolean): Stage {
  const [stage, setStage] = useState<Stage>('raw')

  useEffect(() => {
    setStage('raw')
    if (!parts || reduced) return
    const removed = parts.filter((part) => part.kind === 'removed').length
    const strikeEnd = STRIKE_START_MS + STRIKE_MS + (removed - 1) * strikeStagger(parts)
    const collapseAt = strikeEnd + HOLD_STRUCK_MS
    const timers = [
      setTimeout(() => setStage('strike'), STRIKE_START_MS),
      setTimeout(() => setStage('collapse'), collapseAt),
      setTimeout(() => setStage('final'), collapseAt + COLLAPSE_MS),
    ]
    return () => timers.forEach(clearTimeout)
  }, [parts, reduced])

  return reduced ? 'final' : stage
}

function PolishedLine({ line, index, animate }: { line: string; index: number; animate: boolean }) {
  const mark = line.match(LIST_MARK)
  return (
    <motion.div
      initial={animate ? { opacity: 0, y: 6 } : false}
      animate={{ opacity: 1, y: 0 }}
      transition={{ ...spring.gentle, delay: index * 0.06 }}
      className={line ? undefined : 'h-3'}
    >
      {mark ? (
        <>
          <span className="font-medium text-accent">{mark[1]}</span>
          {mark[2]}
          {mark[3]}
        </>
      ) : (
        line
      )}
    </motion.div>
  )
}

function WithoutAdditions({ parts, stagger }: { parts: DiffPart[]; stagger: number }) {
  let removedIndex = 0
  return (
    <>
      {parts.map((part, index) => {
        if (part.kind === 'same') return <span key={index}>{part.text}</span>
        if (part.kind !== 'removed') return null
        const style = { '--strike-delay': `${removedIndex++ * stagger}ms` } as CSSProperties
        return (
          <del key={index} className="tryit-removed" style={style}>
            {part.text}
          </del>
        )
      })}
    </>
  )
}

export function TryItStep() {
  const { t } = useTranslation()
  const hotkey = useAppStore((s) => s.config.hotkey)
  const hotkeyMode = useAppStore((s) => s.config.hotkey_mode)
  const pipelineState = useAppStore((s) => s.pipelineState)
  const partialTranscript = useAppStore((s) => s.partialTranscript)
  const finalTranscript = useAppStore((s) => s.finalTranscript)
  const polishedText = useAppStore((s) => s.polishedText)
  const resetRecording = useAppStore((s) => s.resetRecording)
  const reduced = useReducedMotion() ?? false
  const recognizesLocally = useAppStore((s) => s.config.stt_provider === LOCAL_QWEN3_PROVIDER)
  const idle = pipelineState === 'idle'
  const [keepsVoiceSample, setKeepsVoiceSample] = useState(false)

  // With local recognition and no voice sample yet, this read-aloud becomes the sample.
  useEffect(() => {
    if (!recognizesLocally || !idle) return
    let active = true
    voiceIsolationStatus()
      .then((status) => {
        if (!active) return
        setKeepsVoiceSample(!status.hasSample)
        if (!status.hasSample) return voiceIsolationEnrollNext(true)
      })
      .catch(() => {})
    return () => {
      active = false
    }
  }, [recognizesLocally, idle])

  // Leaving without reading must not turn a later dictation into the sample.
  useEffect(
    () => () => {
      voiceIsolationEnrollNext(false)?.catch(() => {})
    },
    [],
  )

  // The real pipeline types into whatever has focus. This field receives it, so the keys never
  // reach the buttons of this page; the panels read the results from the pipeline events instead.
  const outputSink = useRef<HTMLTextAreaElement>(null)
  const previousState = useRef(pipelineState)

  useEffect(() => {
    outputSink.current?.focus()
  }, [])

  useEffect(() => {
    const wasIdle = previousState.current === 'idle'
    previousState.current = pipelineState
    if (!wasIdle || pipelineState === 'idle') return
    resetRecording()
    if (outputSink.current) {
      outputSink.current.value = ''
      outputSink.current.focus()
    }
  }, [pipelineState, resetRecording])

  const raw = (finalTranscript || partialTranscript).trim()
  const polished = polishedText.trim()
  const isIdle = pipelineState === 'idle'
  const listening = pipelineState === 'recording'
  const working = !isIdle && !listening

  // Polish streams in pieces, so the animation starts only once the text has stopped changing.
  const parts = useMemo(
    () => (isIdle && raw && polished ? diffText(raw, polished) : null),
    [isIdle, raw, polished],
  )
  const stage = useRevealStage(parts, reduced)
  const hasResult = parts !== null || (isIdle && raw !== '')
  const hint = t(hotkeyMode === 'toggle' ? 'onboarding.tryIt.toggle' : 'onboarding.tryIt.hold', {
    hotkey,
  })
  const listeningHint = t(
    hotkeyMode === 'toggle' ? 'onboarding.tryIt.listeningToggle' : 'onboarding.tryIt.listeningHold',
    { hotkey },
  )

  return (
    <div className="space-y-4">
      <p
        className={`text-center transition-all duration-300 ${
          hasResult
            ? 'text-[12px] leading-relaxed text-text-tertiary'
            : 'text-[17px] leading-[1.8] text-text-primary'
        }`}
      >
        {t('onboarding.tryIt.sentence')}
      </p>

      <div className="flex min-h-[36px] items-center justify-center gap-3 text-[12px] text-text-secondary">
        {listening || working ? (
          <div data-testid={listening ? 'try-it-voice' : undefined}>
            <Waveform bars={28} maxHeight={28} barClassName="bg-accent" idle={working} />
          </div>
        ) : (
          <span className="relative flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full bg-accent text-white">
            {!hasResult && (
              <span
                aria-hidden="true"
                className="tryit-ring absolute inset-0 rounded-full bg-accent"
              />
            )}
            <Mic size={14} className="relative" />
          </span>
        )}
        <span>{listening ? listeningHint : working ? t('onboarding.tryIt.working') : hint}</span>
      </div>

      {keepsVoiceSample && !hasResult && (
        <p className="text-center text-[12px] text-text-tertiary">
          {t('onboarding.tryIt.voiceSample')}
        </p>
      )}

      {raw && !parts && (
        <div className={`${SURFACE} ${working || listening ? 'text-text-secondary' : ''}`}>
          {raw}
        </div>
      )}

      {raw && !parts && isIdle && (
        <p className="text-center text-[12px] text-text-tertiary">
          {t('onboarding.tryIt.noPolish')}
        </p>
      )}

      {parts && stage !== 'final' && (
        <motion.div
          data-testid="try-it-sweep"
          data-stage={stage}
          initial={reduced ? false : { opacity: 0 }}
          animate={{ opacity: 1 }}
          className={SURFACE}
        >
          <WithoutAdditions parts={parts} stagger={strikeStagger(parts)} />
        </motion.div>
      )}

      {parts && stage === 'final' && (
        <>
          <div data-testid="try-it-polished" className={SURFACE}>
            {polished.split('\n').map((line, index) => (
              <PolishedLine key={index} line={line} index={index} animate={!reduced} />
            ))}
          </div>
          <motion.div
            data-testid="try-it-said"
            data-stage="strike"
            initial={reduced ? false : { opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ delay: 0.4, duration: 0.3 }}
            className="whitespace-pre-wrap break-words px-1 text-[12px] leading-relaxed text-text-tertiary"
          >
            <span className="mr-2 font-medium text-text-secondary">
              {t('onboarding.tryIt.raw')}
            </span>
            <WithoutAdditions parts={parts} stagger={0} />
          </motion.div>
        </>
      )}

      <textarea
        ref={outputSink}
        data-testid="try-it-output-sink"
        aria-hidden="true"
        tabIndex={-1}
        className="sr-only"
      />
    </div>
  )
}
