import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { X } from 'lucide-react'
import { addSnippet, listSnippets, removeSnippet, type Snippet } from '../../lib/tauri'

const FIELD =
  'w-full rounded-[8px] border border-border bg-bg-secondary px-3 py-2 text-[13px] text-text-primary outline-none focus:border-border-focus'

/** Phrases that, said by themselves, enter a saved text exactly as written. */
export function SnippetsSection() {
  const { t } = useTranslation()
  const [snippets, setSnippets] = useState<Snippet[]>([])
  const [phrase, setPhrase] = useState('')
  const [text, setText] = useState('')
  const [error, setError] = useState<string | null>(null)

  const reload = useCallback(
    () =>
      Promise.resolve(listSnippets())
        .then((list) => setSnippets(list ?? []))
        .catch(() => {}),
    [],
  )

  useEffect(() => {
    void reload()
  }, [reload])

  const add = async () => {
    setError(null)
    try {
      await addSnippet(phrase.trim(), text.trim())
      setPhrase('')
      setText('')
      await reload()
    } catch (reason) {
      setError(
        String(reason).includes('snippet_duplicate') ? 'snippets.duplicate' : 'snippets.failed',
      )
    }
  }

  const remove = async (id: number) => {
    await removeSnippet(id)
    setSnippets((list) => list.filter((snippet) => snippet.id !== id))
  }

  return (
    <div className="space-y-3">
      <p className="text-[12px] text-text-tertiary">{t('snippets.hint')}</p>
      <div className="space-y-2">
        <input
          type="text"
          value={phrase}
          onChange={(event) => setPhrase(event.target.value)}
          aria-label={t('snippets.phrase')}
          placeholder={t('snippets.phrase')}
          maxLength={60}
          className={FIELD}
        />
        <textarea
          value={text}
          onChange={(event) => setText(event.target.value)}
          aria-label={t('snippets.text')}
          placeholder={t('snippets.text')}
          rows={3}
          maxLength={4000}
          className={`${FIELD} resize-y`}
        />
        <button
          type="button"
          onClick={add}
          disabled={!phrase.trim() || !text.trim()}
          className="rounded-[8px] border border-border bg-bg-secondary px-3 py-2 text-[13px] text-text-primary transition-colors hover:bg-bg-tertiary disabled:cursor-not-allowed disabled:opacity-50"
        >
          {t('snippets.add')}
        </button>
        {error && <p className="text-[12px] text-error">{t(error)}</p>}
      </div>
      {snippets.length > 0 && (
        <ul className="divide-y divide-border rounded-[8px] border border-border">
          {snippets.map((snippet) => (
            <li key={snippet.id} className="flex items-start gap-3 px-3 py-2 text-[13px]">
              <span className="w-[30%] shrink-0 break-words font-medium text-text-primary">
                {snippet.phrase}
              </span>
              <span className="min-w-0 flex-1 whitespace-pre-wrap break-words text-text-secondary">
                {snippet.text}
              </span>
              <button
                type="button"
                onClick={() => remove(snippet.id)}
                aria-label={t('snippets.remove')}
                title={t('snippets.remove')}
                className="grid h-6 w-6 shrink-0 place-items-center rounded-[8px] border-none bg-transparent text-text-tertiary hover:bg-bg-hover hover:text-text-secondary"
              >
                <X size={13} />
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
