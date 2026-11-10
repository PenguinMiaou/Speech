import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Mic } from 'lucide-react'
import { useAppStore } from '../../stores/appStore'
import { listInputDevices, type InputDevice } from '../../lib/tauri'
import { LogoSelect, type LogoOption } from './shared/LogoSelect'
import { Toggle } from './shared/Toggle'

interface Props {
  /** Muting the sound output is available on macOS so far. */
  showMute: boolean
}

export function MicrophoneSection({ showMute }: Props) {
  const { t } = useTranslation()
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const [devices, setDevices] = useState<InputDevice[]>([])

  useEffect(() => {
    let active = true
    const refresh = () => {
      listInputDevices()
        .then((list) => {
          if (active) setDevices(list)
        })
        .catch(() => {})
    }
    refresh()
    window.addEventListener('focus', refresh)
    return () => {
      active = false
      window.removeEventListener('focus', refresh)
    }
  }, [])

  const systemDefault = devices.find((device) => device.is_default)
  const option = (value: string, label: string): LogoOption => ({
    value,
    label,
    logo: null,
    fallback: Mic,
  })
  const options = [
    option(
      '',
      systemDefault
        ? t('settings.systemDefaultMicrophone', { name: systemDefault.name })
        : t('settings.systemDefaultMicrophoneUnnamed'),
    ),
    ...devices.map((device) => option(device.id, device.name)),
  ]
  const chosen = config.input_device
  if (chosen && !devices.some((device) => device.id === chosen)) {
    options.push(option(chosen, t('settings.microphoneNotConnected')))
  }

  return (
    <div className="space-y-3">
      <LogoSelect
        label={t('settings.microphoneInput')}
        value={chosen}
        options={options}
        onChange={(input_device) => updateConfig({ input_device })}
      />
      {showMute && (
        <Toggle
          checked={config.mute_system_audio}
          onChange={(mute_system_audio) => updateConfig({ mute_system_audio })}
          label={t('settings.muteWhileRecording')}
        />
      )}
    </div>
  )
}
