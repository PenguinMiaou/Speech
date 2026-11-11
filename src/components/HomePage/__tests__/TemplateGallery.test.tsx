import { useState } from 'react'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TemplateGallery } from '../TemplateGallery'
import * as tauri from '../../../lib/tauri'

vi.mock('../../../lib/tauri')
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

const weekly = { name: 'Weekly', text: '# Done\n# Next' }
const confirm = vi.fn()
const close = vi.fn()

function Held({ run = false }: { run?: boolean }) {
  const [templates, setTemplates] = useState([weekly])
  const [chosen, setChosen] = useState('')
  return (
    <>
      <output>{chosen}</output>
      <TemplateGallery
        templates={templates}
        onTemplates={setTemplates}
        chosen={chosen}
        onChoose={setChosen}
        onClose={close}
        confirm={run ? { label: 'go', onConfirm: confirm } : undefined}
      />
    </>
  )
}

describe('TemplateGallery', () => {
  beforeEach(() => vi.clearAllMocks())
  afterEach(cleanup)

  it('lays out the templates that come with the app and the user’s own to pick from', () => {
    render(<Held />)
    expect(screen.getByRole('radio', { name: 'meeting.defaultTemplateName' })).toHaveProperty(
      'ariaChecked',
      'true',
    )
    expect(screen.getByRole('radio', { name: 'meeting.standupTemplateName' })).toBeTruthy()
    fireEvent.click(screen.getByRole('radio', { name: 'Weekly' }))
    expect(screen.getByRole('status').textContent).toBe('Weekly')
  })

  it('takes in a file as a template and picks it', async () => {
    vi.mocked(tauri.importMinutesTemplate).mockResolvedValue([
      weekly,
      { name: 'Retro', text: '# Went well' },
    ])
    render(<Held />)
    fireEvent.click(screen.getByRole('button', { name: 'meeting.importTemplate' }))
    await screen.findByRole('radio', { name: 'Retro' })
    expect(screen.getByRole('status').textContent).toBe('Retro')
  })

  it('lets a template be written in the app and picks it', async () => {
    vi.mocked(tauri.saveMinutesTemplate).mockResolvedValue([
      weekly,
      { name: 'Mine', text: '# Topic' },
    ])
    render(<Held />)
    fireEvent.click(screen.getByRole('button', { name: 'meeting.newTemplate' }))
    fireEvent.change(screen.getByLabelText('meeting.templateName'), { target: { value: 'Mine' } })
    fireEvent.change(screen.getByLabelText('meeting.templateText'), {
      target: { value: '# Topic' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'meeting.saveTemplate' }))
    await screen.findByRole('radio', { name: 'Mine' })
    expect(tauri.saveMinutesTemplate).toHaveBeenCalledWith('Mine', '# Topic')
    expect(screen.getByRole('status').textContent).toBe('Mine')
  })

  it('says why a template could not be kept', async () => {
    vi.mocked(tauri.saveMinutesTemplate).mockRejectedValue('template_unnamed')
    render(<Held />)
    fireEvent.click(screen.getByRole('button', { name: 'meeting.newTemplate' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.saveTemplate' }))
    await screen.findByText('meeting.error.template_unnamed')
  })

  it('removes one of the user’s own templates, never one that comes with the app', async () => {
    vi.mocked(tauri.removeMinutesTemplate).mockResolvedValue([])
    render(<Held />)
    fireEvent.click(screen.getByRole('radio', { name: 'Weekly' }))
    fireEvent.click(screen.getByRole('button', { name: 'meeting.removeTemplate Weekly' }))
    await waitFor(() => expect(screen.queryByRole('radio', { name: 'Weekly' })).toBeNull())
    expect(screen.getByRole('status').textContent).toBe('')
    expect(screen.queryByRole('button', { name: /meeting.removeTemplate meeting/ })).toBeNull()
  })

  it('goes ahead with the picked template only when asked to', () => {
    render(<Held run />)
    fireEvent.click(screen.getByRole('radio', { name: 'Weekly' }))
    expect(confirm).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: 'go' }))
    expect(confirm).toHaveBeenCalled()
  })

  it('closes without going ahead', () => {
    render(<Held run />)
    fireEvent.click(screen.getByRole('button', { name: 'meeting.cancel' }))
    expect(close).toHaveBeenCalled()
    expect(confirm).not.toHaveBeenCalled()
  })
})
