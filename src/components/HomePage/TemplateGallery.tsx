import { useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { Check, FilePlus2, FileUp, Trash2, X } from 'lucide-react'
import {
  importMinutesTemplate,
  removeMinutesTemplate,
  saveMinutesTemplate,
  type MinutesTemplate,
} from '../../lib/tauri'

/** The general template that comes with the app. */
export const BUILT_IN = ''
// The templates that come with the app besides the general one. No file can carry these
// names, so they cannot collide with a template of the user's own.
export const OWN = ['chapters', 'oneOnOne', 'interview', 'talk', 'standup']
export const own = (kind: string) => `\u0000${kind}`
export const ownKind = (chosen: string) => OWN.find((kind) => own(kind) === chosen)

const KNOWN_ERRORS = [
  'template_too_large',
  'template_empty',
  'template_unreadable',
  'template_unnamed',
]

/** What the chosen template says, in the words of `t` for one that comes with the app. */
export function templateText(
  t: (key: string) => string,
  templates: MinutesTemplate[],
  chosen: string,
): string {
  return (
    templates.find((entry) => entry.name === chosen)?.text ??
    t(`meeting.${ownKind(chosen) ?? 'default'}Template`)
  )
}

/** A template as a small page: its headings as words, the rest as lines. */
function Page({ text }: { text: string }) {
  const lines = text
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(0, 9)
  return (
    <span
      aria-hidden
      className="flex aspect-[3/4] w-full flex-col gap-[5px] overflow-hidden rounded-[6px] border border-border bg-bg-primary px-2.5 py-3"
    >
      {lines.map((line, index) => {
        const heading = /^#{1,4} (.*)$/.exec(line)
        return heading ? (
          <span
            key={index}
            className="truncate text-[9px] font-semibold leading-[1.2] text-text-primary"
          >
            {heading[1]}
          </span>
        ) : (
          <span
            key={index}
            className="h-[3px] shrink-0 rounded-full bg-border"
            style={{ width: `${55 + ((line.length * 7) % 40)}%` }}
          />
        )
      })}
    </span>
  )
}

/** The minutes templates laid out to pick from, the way a word processor offers new
 * documents: the ones that come with the app, the user's own, and two ways to add one,
 * from a file or written here. With `confirm`, picking is followed by going ahead. */
export function TemplateGallery({
  templates,
  onTemplates,
  chosen,
  onChoose,
  onClose,
  confirm,
  children,
}: {
  templates: MinutesTemplate[]
  onTemplates: (all: MinutesTemplate[]) => void
  chosen: string
  onChoose: (name: string) => void
  onClose: () => void
  confirm?: { label: string; onConfirm: () => void }
  /** What else is asked before going ahead, shown beside the buttons. */
  children?: ReactNode
}) {
  const { t } = useTranslation()
  const [error, setError] = useState<string | null>(null)
  const [writing, setWriting] = useState(false)
  const [name, setName] = useState('')
  const [text, setText] = useState('')

  const fail = (reason: unknown) => {
    const code = KNOWN_ERRORS.find((known) => String(reason).includes(known))
    setError(`meeting.error.${code ?? 'template_unreadable'}`)
  }
  /** Takes the templates as they now are and picks the one that was not there before. */
  const added = (all: MinutesTemplate[] | null, wanted?: string) => {
    if (!all) return
    const known = new Set(templates.map((entry) => entry.name))
    onTemplates(all)
    const fresh = all.find((entry) => entry.name === wanted) ?? all.find((e) => !known.has(e.name))
    if (fresh) onChoose(fresh.name)
  }
  const add = () => {
    setError(null)
    Promise.resolve(importMinutesTemplate())
      .then((all) => added(all))
      .catch(fail)
  }
  const save = () => {
    setError(null)
    Promise.resolve(saveMinutesTemplate(name, text))
      .then((all) => {
        added(all, name.trim())
        setWriting(false)
      })
      .catch(fail)
  }
  const remove = (gone: string) =>
    void Promise.resolve(removeMinutesTemplate(gone))
      .then((all) => {
        onTemplates(all ?? [])
        if (chosen === gone) onChoose(BUILT_IN)
      })
      .catch(() => {})

  const cards = [
    { value: BUILT_IN, label: t('meeting.defaultTemplateName'), mine: false },
    ...OWN.map((kind) => ({
      value: own(kind),
      label: t(`meeting.${kind}TemplateName`),
      mine: false,
    })),
    ...templates.map((entry) => ({ value: entry.name, label: entry.name, mine: true })),
  ]
  const tile =
    'flex aspect-[3/4] w-full cursor-pointer flex-col items-center justify-center gap-2 rounded-[6px] border border-dashed border-border bg-transparent text-[12px] text-text-secondary transition-colors hover:border-accent hover:text-accent'

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/25 px-5">
      <div className="fixed inset-0" onClick={onClose} />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t('meeting.template')}
        className="relative z-10 flex max-h-[82vh] w-full max-w-[720px] flex-col rounded-[10px] border border-border bg-bg-secondary shadow-float"
      >
        <div className="flex items-center justify-between px-5 pb-1 pt-4">
          <h3 className="m-0 text-[15px] font-semibold text-text-primary">
            {t(writing ? 'meeting.newTemplate' : 'meeting.template')}
          </h3>
          <button
            type="button"
            onClick={onClose}
            aria-label={t('meeting.cancel')}
            className="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[6px] border-none bg-transparent text-text-tertiary hover:text-text-primary"
          >
            <X size={15} />
          </button>
        </div>
        {writing ? (
          <div className="flex min-h-0 flex-col gap-2.5 px-5 py-3">
            <input
              autoFocus
              value={name}
              onChange={(event) => setName(event.target.value)}
              aria-label={t('meeting.templateName')}
              placeholder={t('meeting.templateName')}
              className="h-9 rounded-[8px] border border-border bg-bg-primary px-3 text-[13px] text-text-primary outline-none placeholder:text-text-tertiary"
            />
            <textarea
              value={text}
              onChange={(event) => setText(event.target.value)}
              rows={12}
              aria-label={t('meeting.templateText')}
              className="min-h-0 resize-y rounded-[8px] border border-border bg-bg-primary px-3 py-2.5 font-mono text-[12px] leading-relaxed text-text-primary outline-none"
            />
          </div>
        ) : (
          <div
            role="radiogroup"
            aria-label={t('meeting.template')}
            className="grid min-h-0 grid-cols-5 gap-x-4 gap-y-4 overflow-y-auto px-5 py-3"
          >
            <button
              type="button"
              onClick={() => {
                setError(null)
                setName('')
                setText(templateText(t, templates, chosen))
                setWriting(true)
              }}
              className={tile}
            >
              <FilePlus2 size={22} strokeWidth={1.5} />
              {t('meeting.newTemplate')}
            </button>
            <button type="button" onClick={add} className={tile}>
              <FileUp size={22} strokeWidth={1.5} />
              {t('meeting.importTemplate')}
            </button>
            {cards.map((card) => {
              const picked = card.value === chosen
              return (
                <div key={card.value} className="group relative">
                  <button
                    type="button"
                    role="radio"
                    aria-checked={picked}
                    aria-label={card.label}
                    onClick={() => onChoose(card.value)}
                    onDoubleClick={confirm?.onConfirm}
                    className="flex w-full cursor-pointer flex-col gap-1.5 border-none bg-transparent p-0 text-left"
                  >
                    <span
                      className={`relative block w-full rounded-[7px] transition-shadow ${
                        picked
                          ? 'shadow-[0_0_0_2px_var(--color-accent)]'
                          : 'hover:shadow-[0_0_0_1px_var(--color-border)]'
                      }`}
                    >
                      <Page text={templateText(t, templates, card.value)} />
                      {picked && (
                        <span className="absolute bottom-1.5 right-1.5 flex h-4 w-4 items-center justify-center rounded-full bg-accent text-white">
                          <Check size={11} strokeWidth={3} />
                        </span>
                      )}
                    </span>
                    <span
                      className={`truncate text-[12px] ${
                        picked ? 'font-medium text-text-primary' : 'text-text-secondary'
                      }`}
                    >
                      {card.label}
                    </span>
                  </button>
                  {card.mine && (
                    <button
                      type="button"
                      aria-label={`${t('meeting.removeTemplate')} ${card.label}`}
                      title={t('meeting.removeTemplate')}
                      onClick={() => remove(card.value)}
                      className="absolute right-1.5 top-1.5 flex h-5 w-5 cursor-pointer items-center justify-center rounded-[5px] border-none bg-bg-secondary text-text-tertiary opacity-0 transition-opacity hover:text-error focus-visible:opacity-100 group-hover:opacity-100"
                    >
                      <Trash2 size={12} />
                    </button>
                  )}
                </div>
              )
            })}
          </div>
        )}
        {error && <p className="m-0 px-5 text-[12px] text-error">{t(error)}</p>}
        <div className="flex items-center gap-2 px-5 pb-4 pt-2">
          <div className="min-w-0 flex-1">{!writing && children}</div>
          {writing ? (
            <>
              <button type="button" onClick={() => setWriting(false)} className="btn-quiet">
                {t('meeting.back')}
              </button>
              <button type="button" onClick={save} className="btn-record">
                {t('meeting.saveTemplate')}
              </button>
            </>
          ) : (
            confirm && (
              <button type="button" onClick={confirm.onConfirm} className="btn-record">
                {confirm.label}
              </button>
            )
          )}
        </div>
      </div>
    </div>
  )
}
