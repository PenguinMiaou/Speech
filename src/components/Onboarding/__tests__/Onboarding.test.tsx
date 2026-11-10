import React from 'react'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Onboarding } from '../index'
import { saveOnboardingConfig, saveOnboardingCompleted } from '../../../lib/tauri'

const mockStore = {
  onboardingStep: 0,
  setOnboardingStep: vi.fn(),
  setOnboardingCompleted: vi.fn(),
  updateConfig: vi.fn(),
  setConfig: vi.fn(),
  setSavedConfig: vi.fn(),
  config: { stt_provider: 'local-qwen3' },
}

vi.mock('framer-motion', () => ({
  AnimatePresence: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  motion: {
    div: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  },
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

vi.mock('../OnboardingLayout', () => ({
  OnboardingLayout: ({
    children,
    onBack,
    onNext,
    onSkip,
    canNext,
    nextLabel,
    totalSteps,
  }: {
    children: React.ReactNode
    onBack: () => void
    onNext: () => void
    onSkip?: () => void
    canNext: boolean
    nextLabel?: string
    totalSteps: number
  }) => (
    <div>
      <span>total {totalSteps}</span>
      <button type="button" onClick={onBack}>
        Back
      </button>
      <button type="button" onClick={onNext} disabled={!canNext}>
        {nextLabel}
      </button>
      <button type="button" onClick={onSkip}>
        Skip
      </button>
      {children}
    </div>
  ),
}))

vi.mock('../WelcomeStep', () => ({ WelcomeStep: () => <div>Language step</div> }))
vi.mock('../SttSetupStep', () => ({ SttSetupStep: () => <div>Speech step</div> }))
vi.mock('../../Settings/LlmConnectionForm', () => ({
  LlmConnectionForm: () => <div>Polish step</div>,
}))
vi.mock('../PermissionsStep', () => ({ PermissionsStep: () => <div>Permissions step</div> }))
vi.mock('../TryItStep', () => ({ TryItStep: () => <div>Try it step</div> }))

vi.mock('../../../stores/appStore', () => ({
  useAppStore: (selector: (state: typeof mockStore) => unknown) => selector(mockStore),
}))

vi.mock('../../../lib/tauri', () => ({
  saveOnboardingConfig: vi.fn(),
  saveOnboardingCompleted: vi.fn(),
}))

const saved = { stt_provider: 'local-qwen3', saved: true }

describe('Onboarding', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockStore.onboardingStep = 0
    vi.mocked(saveOnboardingConfig).mockResolvedValue(saved as never)
    vi.mocked(saveOnboardingCompleted).mockResolvedValue(undefined)
  })

  afterEach(cleanup)

  it.each([
    [0, 'Language step'],
    [1, 'Speech step'],
    [2, 'Polish step'],
    [3, 'Permissions step'],
    [4, 'Try it step'],
  ])('shows step %i as %s, five steps in total', (step, label) => {
    mockStore.onboardingStep = step
    render(<Onboarding />)
    expect(screen.getByText(label)).toBeTruthy()
    expect(screen.getByText('total 5')).toBeTruthy()
  })

  it('moves from the language step to the speech step', async () => {
    render(<Onboarding />)
    fireEvent.click(screen.getByText('onboarding.layout.next'))
    await waitFor(() => expect(mockStore.setOnboardingStep).toHaveBeenCalledWith(1))
  })

  it.each([1, 2, 3, 4])('never blocks the next button on step %i', (step) => {
    mockStore.onboardingStep = step
    render(<Onboarding />)
    const next = screen.getByRole('button', {
      name: step === 4 ? 'onboarding.steps.getStarted' : 'onboarding.layout.next',
    }) as HTMLButtonElement
    expect(next.disabled).toBe(false)
  })

  // A model install can finish on a later step and saves on top of savedConfig, so what each step
  // saved must already be there or the install would write the earlier values back.
  it('records each saved step as the saved config', async () => {
    mockStore.onboardingStep = 2
    render(<Onboarding />)
    fireEvent.click(screen.getByText('onboarding.layout.next'))
    await waitFor(() => expect(mockStore.setSavedConfig).toHaveBeenCalledWith(saved))
    expect(mockStore.setOnboardingStep).toHaveBeenCalledWith(3)
  })

  it('finishes from the last step', async () => {
    mockStore.onboardingStep = 4
    render(<Onboarding />)
    fireEvent.click(screen.getByText('onboarding.steps.getStarted'))
    await waitFor(() => expect(mockStore.setOnboardingCompleted).toHaveBeenCalledWith(true))
    expect(saveOnboardingCompleted).toHaveBeenCalledTimes(1)
    expect(mockStore.setConfig).toHaveBeenCalledWith(saved)
  })

  it('skip finishes the whole setup from any step', async () => {
    mockStore.onboardingStep = 1
    render(<Onboarding />)
    fireEvent.click(screen.getByText('Skip'))
    await waitFor(() => expect(mockStore.setOnboardingCompleted).toHaveBeenCalledWith(true))
    expect(saveOnboardingCompleted).toHaveBeenCalledTimes(1)
  })

  it('stays open and reports when the final save fails', async () => {
    mockStore.onboardingStep = 4
    vi.mocked(saveOnboardingConfig).mockRejectedValue(new Error('disk full'))
    render(<Onboarding />)
    fireEvent.click(screen.getByText('onboarding.steps.getStarted'))
    await screen.findByRole('alert')
    expect(mockStore.setOnboardingCompleted).not.toHaveBeenCalled()
    expect(saveOnboardingCompleted).not.toHaveBeenCalled()
  })
})
