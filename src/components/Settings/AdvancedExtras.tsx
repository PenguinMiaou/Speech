import { useTranslation } from 'react-i18next'
import {
  isMacPlatform,
  isWindowsPlatform,
  useAppStore,
  type PasteShortcut,
  type WindowsSendInputNewlineMode,
} from '../../stores/appStore'
import { LogoSelect } from './shared/LogoSelect'
import { Toggle } from './shared/Toggle'

const NUMBER =
  'h-8 w-24 rounded-[8px] border border-border bg-bg-secondary px-2 text-right text-[13px] text-text-primary outline-none focus:border-border-focus'

function whole(text: string, least: number, most: number): number {
  const value = Math.floor(Number(text))
  return Number.isFinite(value) ? Math.min(most, Math.max(least, value)) : least
}

/** Output and history choices the backend already honours. */
export function AdvancedExtras() {
  const { t } = useTranslation()
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const pasting = config.output_mode === 'clipboard'

  return (
    <>
      <Toggle
        checked={config.streaming_insert_enabled}
        onChange={(checked) => updateConfig({ streaming_insert_enabled: checked })}
        label={t('settings.streamingInsert')}
      />
      {pasting && (
        <Toggle
          checked={config.restore_clipboard_after_paste}
          onChange={(checked) => updateConfig({ restore_clipboard_after_paste: checked })}
          label={t('settings.restoreClipboardAfterPaste')}
        />
      )}
      {pasting && !isMacPlatform() && (
        <Row label={t('settings.pasteShortcut')}>
          <LogoSelect
            compact
            label={t('settings.pasteShortcut')}
            value={config.paste_shortcut}
            options={[
              { value: 'ctrlV', label: t('settings.pasteShortcutCtrlV') },
              { value: 'ctrlShiftV', label: t('settings.pasteShortcutCtrlShiftV') },
              { value: 'shiftInsert', label: t('settings.pasteShortcutShiftInsert') },
            ]}
            onChange={(value) => updateConfig({ paste_shortcut: value as PasteShortcut })}
          />
        </Row>
      )}
      {isWindowsPlatform() && (
        <Row label={t('settings.windowsSendInputNewlineMode')}>
          <LogoSelect
            compact
            label={t('settings.windowsSendInputNewlineMode')}
            value={config.windows_sendinput_newline_mode}
            options={[
              { value: 'enter', label: t('settings.windowsSendInputNewlineEnter') },
              { value: 'shiftEnter', label: t('settings.windowsSendInputNewlineShiftEnter') },
              { value: 'crlf', label: t('settings.windowsSendInputNewlineCrlf') },
            ]}
            onChange={(value) =>
              updateConfig({
                windows_sendinput_newline_mode: value as WindowsSendInputNewlineMode,
              })
            }
          />
        </Row>
      )}
      {config.history_enabled && (
        <>
          <Row label={t('settings.historyRetentionDays')}>
            <input
              type="number"
              min={0}
              max={3650}
              value={config.history_retention_days}
              onChange={(event) =>
                updateConfig({ history_retention_days: whole(event.target.value, 0, 3650) })
              }
              aria-label={t('settings.historyRetentionDays')}
              className={NUMBER}
            />
          </Row>
          <Row label={t('settings.historyMaxEntries')}>
            <input
              type="number"
              min={1}
              max={5000}
              value={config.history_max_entries}
              onChange={(event) =>
                updateConfig({ history_max_entries: whole(event.target.value, 1, 5000) })
              }
              aria-label={t('settings.historyMaxEntries')}
              className={NUMBER}
            />
          </Row>
        </>
      )}
    </>
  )
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-[13px] text-text-primary">{label}</span>
      <div className="w-44 shrink-0 text-right">{children}</div>
    </div>
  )
}
