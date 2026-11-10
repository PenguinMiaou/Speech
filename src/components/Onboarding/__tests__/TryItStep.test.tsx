import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useAppStore } from '../../../stores/appStore'
import { TryItStep } from '../TryItStep'
import * as tauri from '../../../lib/tauri'

vi.mock('../../../lib/tauri')

function voiceSample(hasSample: boolean) {
  vi.mocked(tauri.voiceIsolationStatus).mockResolvedValue({
    modelInstalled: false,
    downloading: false,
    hasSample,
    enrollmentPending: false,
    enrollmentError: null,
    modelBytes: 0,
  })
}

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, params?: { hotkey?: string }) =>
      params?.hotkey ? `${key} ${params.hotkey}` : key,
  }),
}))

let reduceMotion = false
vi.mock('framer-motion', async (original) => ({
  ...(await original<typeof import('framer-motion')>()),
  useReducedMotion: () => reduceMotion,
}))

function setPipeline(state: Partial<ReturnType<typeof useAppStore.getState>>) {
  act(() => useAppStore.setState(state))
}

describe('TryItStep', () => {
  beforeEach(() => {
    const { config } = useAppStore.getState()
    useAppStore.setState({
      pipelineState: 'idle',
      partialTranscript: '',
      finalTranscript: '',
      polishedText: '',
      config: { ...config, hotkey: 'Alt+Space', hotkey_mode: 'hold', stt_provider: 'groq-whisper' },
    })
    vi.clearAllMocks()
    voiceSample(false)
    vi.mocked(tauri.voiceIsolationEnrollNext).mockResolvedValue()
  })

  function recognizeLocally() {
    const { config } = useAppStore.getState()
    useAppStore.setState({ config: { ...config, stt_provider: 'local-qwen3' } })
  }

  it('keeps the read-aloud as the voice sample for local recognition, and says so', async () => {
    recognizeLocally()
    const { unmount } = render(<TryItStep />)
    await screen.findByText('onboarding.tryIt.voiceSample')
    expect(tauri.voiceIsolationEnrollNext).toHaveBeenCalledWith(true)
    unmount()
    expect(tauri.voiceIsolationEnrollNext).toHaveBeenLastCalledWith(false)
  })

  it('leaves an existing voice sample alone', async () => {
    recognizeLocally()
    voiceSample(true)
    render(<TryItStep />)
    await waitFor(() => expect(tauri.voiceIsolationStatus).toHaveBeenCalled())
    expect(tauri.voiceIsolationEnrollNext).not.toHaveBeenCalledWith(true)
    expect(screen.queryByText('onboarding.tryIt.voiceSample')).toBeNull()
  })

  it('keeps no voice sample when recognition is not local', async () => {
    render(<TryItStep />)
    expect(tauri.voiceIsolationStatus).not.toHaveBeenCalled()
    expect(screen.queryByText('onboarding.tryIt.voiceSample')).toBeNull()
  })

  afterEach(() => {
    cleanup()
    vi.useRealTimers()
    reduceMotion = false
  })

  it('shows the sentence to read and the hold-to-talk hotkey', () => {
    render(<TryItStep />)
    expect(screen.getByText('onboarding.tryIt.sentence')).toBeTruthy()
    expect(screen.getByText('onboarding.tryIt.hold Alt+Space')).toBeTruthy()
  })

  it('explains a hotkey pressed once to start and once to stop', () => {
    const { config } = useAppStore.getState()
    useAppStore.setState({ config: { ...config, hotkey_mode: 'toggle' } })
    render(<TryItStep />)
    expect(screen.getByText('onboarding.tryIt.toggle Alt+Space')).toBeTruthy()
  })

  it('shows a live voice meter only while recording', () => {
    render(<TryItStep />)
    expect(screen.queryByTestId('try-it-voice')).toBeNull()
    setPipeline({ pipelineState: 'recording' })
    expect(screen.getByTestId('try-it-voice')).toBeTruthy()
    expect(screen.getByText('onboarding.tryIt.listeningHold Alt+Space')).toBeTruthy()
  })

  it('crosses out what polish removed before the clean text settles in', () => {
    vi.useFakeTimers()
    render(<TryItStep />)
    setPipeline({ finalTranscript: '嗯，两点出发', polishedText: '三点出发' })

    expect(screen.queryByTestId('try-it-polished')).toBeNull()
    const sweep = screen.getByTestId('try-it-sweep')
    expect(sweep.dataset.stage).toBe('raw')
    expect(sweep.querySelector('del')?.textContent).toBe('嗯，两')

    act(() => vi.advanceTimersByTime(600))
    expect(sweep.dataset.stage).toBe('strike')
    act(() => vi.advanceTimersByTime(1000))
    expect(sweep.dataset.stage).toBe('collapse')

    act(() => vi.advanceTimersByTime(600))
    expect(screen.queryByTestId('try-it-sweep')).toBeNull()
    expect(screen.getByTestId('try-it-polished').textContent).toBe('三点出发')
  })

  it('keeps what was said, crossed out, next to the final text', () => {
    vi.useFakeTimers()
    render(<TryItStep />)
    setPipeline({ finalTranscript: '嗯，两点出发', polishedText: '三点出发' })
    act(() => vi.advanceTimersByTime(5000))

    const said = screen.getByTestId('try-it-said')
    expect(said.textContent).toContain('onboarding.tryIt.raw')
    expect(said.querySelector('del')?.textContent).toBe('嗯，两')
  })

  it('goes straight to the final text with reduced motion', () => {
    reduceMotion = true
    render(<TryItStep />)
    setPipeline({ finalTranscript: '嗯，两点出发', polishedText: '三点出发' })
    expect(screen.queryByTestId('try-it-sweep')).toBeNull()
    expect(screen.getByTestId('try-it-polished').textContent).toBe('三点出发')
  })

  it('lays out a polished list line by line and marks its numbers', () => {
    reduceMotion = true
    render(<TryItStep />)
    setPipeline({ finalTranscript: '第一报告第二预算', polishedText: '出发\n\n1. 报告\n2. 预算' })

    const lines = Array.from(screen.getByTestId('try-it-polished').children)
    expect(lines.map((line) => line.textContent)).toEqual(['出发', '', '1. 报告', '2. 预算'])
    expect(lines[2].querySelector('.text-accent')?.textContent).toBe('1.')
  })

  it('waits for the pipeline to finish before starting the reveal', () => {
    vi.useFakeTimers()
    render(<TryItStep />)
    setPipeline({ pipelineState: 'polishing' })
    setPipeline({ finalTranscript: '嗯，两点出发', polishedText: '三' })
    act(() => vi.advanceTimersByTime(5000))
    expect(screen.queryByTestId('try-it-sweep')).toBeNull()
    expect(screen.queryByTestId('try-it-polished')).toBeNull()
    expect(screen.getByText('onboarding.tryIt.working')).toBeTruthy()
  })

  it('shows recognized words while they are still coming in', () => {
    render(<TryItStep />)
    setPipeline({ pipelineState: 'recording' })
    setPipeline({ partialTranscript: '明天' })
    expect(screen.getByText('明天')).toBeTruthy()
  })

  it('says so when recognition finished without a polish result', () => {
    render(<TryItStep />)
    setPipeline({ pipelineState: 'polishing' })
    setPipeline({ finalTranscript: '明天出发' })
    expect(screen.queryByText('onboarding.tryIt.noPolish')).toBeNull()

    setPipeline({ pipelineState: 'idle' })
    expect(screen.getByText('onboarding.tryIt.noPolish')).toBeTruthy()
    expect(document.querySelector('del')).toBeNull()
    expect(screen.getByText('明天出发')).toBeTruthy()
  })

  it('clears the previous attempt when a new recording starts', () => {
    render(<TryItStep />)
    reduceMotion = true
    setPipeline({ finalTranscript: '上一次', polishedText: '上一次的结果' })
    expect(screen.getByText('上一次的结果')).toBeTruthy()

    setPipeline({ pipelineState: 'preparing' })

    expect(screen.queryByText('上一次的结果')).toBeNull()
    expect(useAppStore.getState().finalTranscript).toBe('')
  })

  // The real pipeline types into whatever has focus. Without a text field to receive it the keys
  // reach the buttons of this page.
  it('keeps focus in a text field the output can land in', () => {
    render(<TryItStep />)
    const sink = screen.getByTestId('try-it-output-sink') as HTMLTextAreaElement
    expect(document.activeElement).toBe(sink)

    sink.value = 'typed by the previous attempt'
    sink.blur()
    setPipeline({ pipelineState: 'preparing' })

    expect(document.activeElement).toBe(sink)
    expect(sink.value).toBe('')
  })

  describe('three demonstrations', () => {
    const tab = (name: string) => screen.getByRole('tab', { name })

    function allowSelection(allowed: boolean) {
      const { config } = useAppStore.getState()
      useAppStore.setState({ config: { ...config, selected_text_enabled: allowed } })
    }

    it('starts on the first of three and shows the passage of the chosen one', () => {
      render(<TryItStep />)
      expect(screen.getAllByRole('tab')).toHaveLength(3)
      expect(tab('onboarding.tryIt.demos.cleanup').getAttribute('aria-selected')).toBe('true')

      fireEvent.click(tab('onboarding.tryIt.demos.long'))

      expect(screen.getByText('onboarding.tryIt.longPassage')).toBeTruthy()
      expect(screen.queryByText('onboarding.tryIt.sentence')).toBeNull()
    })

    it('offers the next demonstration after a result and starts it clean', () => {
      reduceMotion = true
      render(<TryItStep />)
      expect(screen.queryByText('onboarding.tryIt.next')).toBeNull()
      setPipeline({ finalTranscript: '嗯，两点出发', polishedText: '三点出发' })

      fireEvent.click(screen.getByText('onboarding.tryIt.next'))

      expect(tab('onboarding.tryIt.demos.long').getAttribute('aria-selected')).toBe('true')
      expect(useAppStore.getState().finalTranscript).toBe('')
      expect(screen.queryByText('三点出发')).toBeNull()
    })

    it('selects a draft to rewrite by voice in the third', () => {
      allowSelection(true)
      render(<TryItStep />)
      fireEvent.click(tab('onboarding.tryIt.demos.rewrite'))

      const draft = screen.getByLabelText('onboarding.tryIt.demos.rewrite') as HTMLTextAreaElement
      expect(draft.value).toBe('onboarding.tryIt.rewriteDraft')
      expect(document.activeElement).toBe(draft)
      expect(draft.selectionEnd - draft.selectionStart).toBe(draft.value.length)
      expect(screen.getByText('onboarding.tryIt.rewriteHint Alt+Space')).toBeTruthy()
    })

    it('keeps the draft in place when the recording starts', () => {
      allowSelection(true)
      render(<TryItStep />)
      fireEvent.click(tab('onboarding.tryIt.demos.rewrite'))
      setPipeline({ pipelineState: 'preparing' })

      const draft = screen.getByLabelText('onboarding.tryIt.demos.rewrite') as HTMLTextAreaElement
      expect(draft.value).toBe('onboarding.tryIt.rewriteDraft')
    })

    it('shows the spoken instruction and can put the draft back', () => {
      allowSelection(true)
      render(<TryItStep />)
      fireEvent.click(tab('onboarding.tryIt.demos.rewrite'))
      const draft = screen.getByLabelText('onboarding.tryIt.demos.rewrite') as HTMLTextAreaElement
      draft.value = 'A formal sentence.'
      setPipeline({ finalTranscript: '改得正式一点', polishedText: 'A formal sentence.' })
      expect(screen.getByText('改得正式一点')).toBeTruthy()

      fireEvent.click(screen.getByText('onboarding.tryIt.again'))

      expect(draft.value).toBe('onboarding.tryIt.rewriteDraft')
      expect(document.activeElement).toBe(draft)
      expect(useAppStore.getState().finalTranscript).toBe('')
    })

    it('asks to switch on reading the selection, and saves the choice', async () => {
      allowSelection(false)
      vi.mocked(tauri.updateConfig).mockResolvedValue()
      render(<TryItStep />)
      fireEvent.click(tab('onboarding.tryIt.demos.rewrite'))
      expect(screen.queryByText('onboarding.tryIt.rewriteHint Alt+Space')).toBeNull()

      fireEvent.click(screen.getByRole('switch'))

      expect(useAppStore.getState().config.selected_text_enabled).toBe(true)
      expect(vi.mocked(tauri.updateConfig).mock.calls[0][0].selected_text_enabled).toBe(true)
      expect(screen.getByText('onboarding.tryIt.rewriteHint Alt+Space')).toBeTruthy()
      expect(document.activeElement).toBe(screen.getByLabelText('onboarding.tryIt.demos.rewrite'))
    })

    it('takes no voice sample from the short instruction of the third', async () => {
      recognizeLocally()
      allowSelection(true)
      render(<TryItStep />)
      await screen.findByText('onboarding.tryIt.voiceSample')
      vi.mocked(tauri.voiceIsolationEnrollNext).mockClear()

      fireEvent.click(tab('onboarding.tryIt.demos.rewrite'))

      expect(tauri.voiceIsolationEnrollNext).toHaveBeenCalledWith(false)
      expect(tauri.voiceIsolationEnrollNext).not.toHaveBeenCalledWith(true)
    })
  })
})
