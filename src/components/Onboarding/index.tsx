import { AnimatePresence, motion } from 'framer-motion'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useAppStore } from '../../stores/appStore'
import { saveOnboardingCompleted, saveOnboardingConfig } from '../../lib/tauri'
import { OnboardingLayout } from './OnboardingLayout'
import { WelcomeStep } from './WelcomeStep'
import { SttSetupStep } from './SttSetupStep'
import { LlmConnectionForm } from '../Settings/LlmConnectionForm'
import { PermissionsStep } from './PermissionsStep'
import { TryItStep } from './TryItStep'
import { slideRight } from '../../lib/animations'

const STEPS = [
  {
    titleKey: 'onboarding.steps.welcome',
    subtitleKey: 'onboarding.steps.welcomeSub',
    Body: WelcomeStep,
  },
  {
    titleKey: 'onboarding.steps.speechRecognition',
    subtitleKey: 'onboarding.steps.speechRecognitionSub',
    Body: SttSetupStep,
  },
  {
    titleKey: 'onboarding.steps.aiPolish',
    subtitleKey: 'onboarding.steps.aiPolishSub',
    Body: LlmConnectionForm,
  },
  {
    titleKey: 'onboarding.steps.permissions',
    subtitleKey: 'onboarding.steps.permissionsSub',
    Body: PermissionsStep,
  },
  {
    titleKey: 'onboarding.steps.tryIt',
    subtitleKey: 'onboarding.steps.tryItSub',
    Body: TryItStep,
    wide: true,
  },
]

export function Onboarding() {
  const { t } = useTranslation()
  const [saveFailed, setSaveFailed] = useState(false)
  const step = useAppStore((s) => s.onboardingStep)
  const setStep = useAppStore((s) => s.setOnboardingStep)
  const setOnboardingCompleted = useAppStore((s) => s.setOnboardingCompleted)
  const setConfig = useAppStore((s) => s.setConfig)
  const setSavedConfig = useAppStore((s) => s.setSavedConfig)
  const config = useAppStore((s) => s.config)

  const lastStep = STEPS.length - 1
  const { titleKey, subtitleKey, Body, wide } = STEPS[step] as (typeof STEPS)[number] & {
    wide?: boolean
  }

  // A model install can finish on a later step and saves on top of savedConfig, so each step's
  // result has to be recorded there for the install not to write earlier values back.
  const saveConfig = async () => {
    setSaveFailed(false)
    try {
      const saved = await saveOnboardingConfig(config)
      setSavedConfig(saved)
      return saved
    } catch (error) {
      setSaveFailed(true)
      throw error
    }
  }

  const finish = async () => {
    try {
      const saved = await saveConfig()
      await saveOnboardingCompleted()
      setConfig(saved)
      setOnboardingCompleted(true)
    } catch {
      setSaveFailed(true)
    }
  }

  const goTo = async (target: number) => {
    try {
      await saveConfig()
    } catch {
      // The alert stays visible and the final step saves again.
    }
    setStep(target)
  }

  return (
    <OnboardingLayout
      step={step}
      totalSteps={STEPS.length}
      title={t(titleKey)}
      subtitle={t(subtitleKey)}
      canNext
      canBack={step > 0}
      nextLabel={step === lastStep ? t('onboarding.steps.getStarted') : t('onboarding.layout.next')}
      onNext={step === lastStep ? finish : () => goTo(step + 1)}
      onBack={() => goTo(step - 1)}
      onSkip={finish}
      wide={wide}
    >
      {saveFailed && (
        <p role="alert" className="mb-3 text-sm text-error">
          {t('common.saveFailed')}
        </p>
      )}
      <AnimatePresence mode="wait">
        <motion.div
          key={step}
          variants={slideRight}
          initial="initial"
          animate="animate"
          exit="exit"
          transition={{ duration: 0.2 }}
        >
          <Body />
        </motion.div>
      </AnimatePresence>
    </OnboardingLayout>
  )
}
