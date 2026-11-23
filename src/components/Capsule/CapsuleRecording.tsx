import { useTranslation } from 'react-i18next'
import { motion, useReducedMotion } from 'framer-motion'
import { MicOff, X } from 'lucide-react'
import { useAppStore } from '../../stores/appStore'
import { abortRecording } from '../../lib/tauri'
import { Waveform } from './Waveform'
import { DurationTimer } from './DurationTimer'
import { TranslateTargetChip } from './TranslateTargetChip'

export function CapsuleRecording() {
  const { t } = useTranslation()
  const reduced = useReducedMotion()
  const micSilent = useAppStore((s) => s.micSilent)

  const handleCancel = async (e: React.MouseEvent) => {
    e.stopPropagation()
    try {
      await abortRecording()
    } catch (err) {
      console.error('Failed to abort recording:', err)
    }
  }

  const stopPointerPropagation = (e: React.PointerEvent) => {
    e.stopPropagation()
  }

  return (
    <motion.div className="relative z-10 flex items-center gap-1.5 h-9 pl-3 pr-2">
      {/* The S of the wordmark, with a slow metallic sheen */}
      <span
        aria-hidden="true"
        className={`capsule-mark flex-shrink-0${reduced ? ' capsule-mark-still' : ''}`}
      >
        S
      </span>
      {micSilent ? (
        <span
          role="alert"
          aria-label={t('capsule.errors.micSilent')}
          title={t('capsule.errors.micSilent')}
          className="flex flex-1 items-center justify-center text-red-400"
        >
          <MicOff size={14} />
        </span>
      ) : (
        <Waveform />
      )}
      <TranslateTargetChip />
      <DurationTimer />
      <button
        onPointerDown={stopPointerPropagation}
        onPointerUp={stopPointerPropagation}
        onClick={handleCancel}
        aria-label={t('capsule.cancelRecording')}
        className="flex-shrink-0 -ml-1 p-1 rounded-full text-white/70 hover:text-white hover:bg-white/15 transition-colors bg-transparent border-none cursor-pointer"
      >
        <X size={12} />
      </button>
    </motion.div>
  )
}
