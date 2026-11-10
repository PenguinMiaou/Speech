import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { SnippetsSection } from '../SnippetsSection'
import * as tauri from '../../../lib/tauri'

vi.mock('../../../lib/tauri')
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

const EMAIL = { id: 1, phrase: '我的邮箱', text: 'someone@example.com' }

describe('SnippetsSection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(tauri.listSnippets).mockResolvedValue([EMAIL])
  })
  afterEach(cleanup)

  it('lists the saved phrases with their text', async () => {
    render(<SnippetsSection />)
    expect(await screen.findByText('我的邮箱')).toBeTruthy()
    expect(screen.getByText('someone@example.com')).toBeTruthy()
  })

  it('saves a new phrase and shows it', async () => {
    vi.mocked(tauri.addSnippet).mockResolvedValue()
    render(<SnippetsSection />)
    await screen.findByText('我的邮箱')
    vi.mocked(tauri.listSnippets).mockResolvedValue([
      EMAIL,
      { id: 2, phrase: '会议链接', text: 'https://example.com/meet' },
    ])

    fireEvent.change(screen.getByLabelText('snippets.phrase'), { target: { value: '会议链接' } })
    fireEvent.change(screen.getByLabelText('snippets.text'), {
      target: { value: 'https://example.com/meet' },
    })
    fireEvent.click(screen.getByText('snippets.add'))

    await waitFor(() =>
      expect(tauri.addSnippet).toHaveBeenCalledWith('会议链接', 'https://example.com/meet'),
    )
    expect(await screen.findByText('会议链接')).toBeTruthy()
    expect((screen.getByLabelText('snippets.phrase') as HTMLInputElement).value).toBe('')
  })

  it('does not save with either part missing', async () => {
    render(<SnippetsSection />)
    await screen.findByText('我的邮箱')
    expect((screen.getByText('snippets.add') as HTMLButtonElement).disabled).toBe(true)
  })

  it('says so when the phrase is already taken', async () => {
    vi.mocked(tauri.addSnippet).mockRejectedValue('snippet_duplicate')
    render(<SnippetsSection />)
    await screen.findByText('我的邮箱')
    fireEvent.change(screen.getByLabelText('snippets.phrase'), { target: { value: '我的邮箱' } })
    fireEvent.change(screen.getByLabelText('snippets.text'), { target: { value: 'x' } })
    fireEvent.click(screen.getByText('snippets.add'))
    expect(await screen.findByText('snippets.duplicate')).toBeTruthy()
  })

  it('removes a phrase', async () => {
    vi.mocked(tauri.removeSnippet).mockResolvedValue()
    render(<SnippetsSection />)
    await screen.findByText('我的邮箱')
    fireEvent.click(screen.getByRole('button', { name: 'snippets.remove' }))
    await waitFor(() => expect(tauri.removeSnippet).toHaveBeenCalledWith(1))
    await waitFor(() => expect(screen.queryByText('我的邮箱')).toBeNull())
  })
})
