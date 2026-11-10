import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { DiagnosticsSection } from '../DiagnosticsSection'
import * as tauri from '../../../lib/tauri'

vi.mock('../../../lib/tauri')
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

const row = (id: string, status: string, message: string) => ({
  id,
  status,
  message,
  action: null,
  lastCheckedAt: '',
})

describe('DiagnosticsSection', () => {
  afterEach(cleanup)

  it('checks only when asked, then lists each part with its state', async () => {
    vi.mocked(tauri.getSystemDiagnostics).mockResolvedValue({
      checkedAt: '',
      rows: [row('microphone', 'ok', 'Built-in mic'), row('hotkey', 'error', 'Not registered')],
    } as never)
    render(<DiagnosticsSection />)
    expect(tauri.getSystemDiagnostics).not.toHaveBeenCalled()

    fireEvent.click(screen.getByText('settings.refreshDiagnostics'))

    expect(await screen.findByText('settings.diagnosticMicrophone')).toBeTruthy()
    expect(screen.getByText('settings.diagnosticStatusOk')).toBeTruthy()
    expect(screen.getByText('settings.diagnosticStatusError')).toBeTruthy()
    expect(screen.getByText('Not registered')).toBeTruthy()
    // A part that is fine needs no explanation.
    expect(screen.queryByText('Built-in mic')).toBeNull()
  })

  it('says so when the check itself fails', async () => {
    vi.mocked(tauri.getSystemDiagnostics).mockRejectedValue('nope')
    render(<DiagnosticsSection />)
    fireEvent.click(screen.getByText('settings.refreshDiagnostics'))
    await waitFor(() => expect(screen.getByText('settings.diagnosticsUnavailable')).toBeTruthy())
  })
})
