import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { FileAudio } from 'lucide-react'
import {
  copyText,
  onFileDropped,
  onFileTranscribeProgress,
  transcribeFile,
  type FileTranscribeProgress,
} from '../../lib/tauri'

const KNOWN_ERRORS = [
  'file_not_audio',
  'file_unreadable',
  'file_too_long',
  'local_model_not_installed',
  'busy',
]

/** Drop an audio or video file on the window and the local model turns it into text. */
export function FileTranscribeCard() {
  const { t } = useTranslation()
  const [progress, setProgress] = useState<FileTranscribeProgress | null>(null)
  const [text, setText] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [working, setWorking] = useState(false)
  const busy = useRef(false)

  useEffect(() => {
    const start = (paths: string[]) => {
      if (busy.current || paths.length === 0) return
      busy.current = true
      setWorking(true)
      setProgress(null)
      setText(null)
      setError(null)
      Promise.resolve(transcribeFile(paths[0]))
        .then((result) => setText(result ?? ''))
        .catch((reason) => {
          const code = KNOWN_ERRORS.find((known) => String(reason).includes(known))
          setError(`fileTranscribe.error.${code ?? 'failed'}`)
        })
        .finally(() => {
          busy.current = false
          setWorking(false)
        })
    }
    const dropped = Promise.resolve(onFileDropped(start))
    const reported = Promise.resolve(onFileTranscribeProgress(setProgress))
    return () => {
      void dropped.then((stop) => stop?.())
      void reported.then((stop) => stop?.())
    }
  }, [])

  return (
    <div className="rounded-[18px] p-5 jelly-card">
      <div className="flex items-center gap-2 text-[13px] text-text-secondary">
        <FileAudio size={15} className="shrink-0 text-text-tertiary" />
        <span>
          {working
            ? progress && progress.total > 0
              ? t('fileTranscribe.working', { done: progress.done, total: progress.total })
              : t('fileTranscribe.reading')
            : t('fileTranscribe.drop')}
        </span>
      </div>
      {error && <p className="mt-2 text-[12px] text-error">{t(error)}</p>}
      {text !== null && (
        <div className="mt-3 space-y-2">
          <textarea
            readOnly
            value={text}
            rows={6}
            aria-label={t('fileTranscribe.result')}
            className="w-full resize-y rounded-[10px] border border-border bg-bg-secondary px-3 py-2 text-[13px] leading-relaxed text-text-primary outline-none"
          />
          <button
            type="button"
            onClick={() => void copyText(text)}
            className="rounded-[10px] border border-border bg-bg-secondary px-3 py-1.5 text-[12px] text-text-primary transition-colors hover:bg-bg-tertiary"
          >
            {t('fileTranscribe.copy')}
          </button>
        </div>
      )}
    </div>
  )
}
