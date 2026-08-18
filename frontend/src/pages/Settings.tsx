import { useState } from 'react'
import { useNavigate } from 'react-router-dom'

import { Button, Div, Input, Label, ToggleSwitch } from '../components/primitives'
import { ThemePreview } from '../components/ThemePreview'
import { TypewriterLabel } from '../components/TypewriterLabel'
import { useSettings } from '../context/useSettings'
import { useDocumentTitle } from '../hooks/useDocumentTitle'
import { requestNotificationPermission } from '../notifications'

/** The browser's local UTC offset in whole hours, used to prefill the timezone field. */
function browserTimezoneOffsetHours(): number {
  return -new Date().getTimezoneOffset() / 60
}

/** Matches the backend's `SettingsStore::set_settings` range check. */
const MIN_TIMEZONE = -12
const MAX_TIMEZONE = 14

function Settings() {
  useDocumentTitle('Settings')
  const navigate = useNavigate()
  const { settings, setSettings } = useSettings()
  const [name, setName] = useState(settings?.name ?? '')
  // Kept as the raw text the user's typing, not round-tripped through `Number`/`String`
  // on every keystroke — doing that snapped a lone "-" back to "0" before a digit could
  // follow it, since `Number('-')` is `NaN`. Parsed to a number only once it's actually
  // needed (on save).
  const [timezoneText, setTimezoneText] = useState(String(settings?.timezone ?? browserTimezoneOffsetHours()))
  const [notificationsEnabled, setNotificationsEnabled] = useState(settings?.notifications_enabled ?? false)

  // Requesting permission has to happen on the actual toggle-on gesture, browsers
  // ignore `Notification.requestPermission()` calls outside a user interaction. If it
  // comes back denied (or the browser doesn't support notifications at all), the toggle
  // reflects that instead of showing on for a permission that was never actually granted.
  const onToggleNotifications = async (enabled: boolean) => {
    if (!enabled) {
      setNotificationsEnabled(false)
      return
    }

    setNotificationsEnabled(await requestNotificationPermission())
  }

  const onBack = () => navigate('/')

  const onSave = async () => {
    const trimmedName = name.trim()
    if (!trimmedName) return

    const timezone = Number(timezoneText)
    if (!Number.isInteger(timezone) || timezone < MIN_TIMEZONE || timezone > MAX_TIMEZONE) return

    await setSettings({ name: trimmedName, timezone, notifications_enabled: notificationsEnabled })
    navigate('/')
  }

  return (
    <Div className="page center vbox" style={{ gap: 16 }}>
      <TypewriterLabel text='[ Settings ]' charIntervalMs={30}/>
      <Div className="panel vbox" style={{ gap: 24 }}>
        <Div className="vbox" style={{ gap: 4 }}>
          <Input text={name} onChanged={setName} placeholder="Enter your name" />
          <Label variant="secondary" text="This name will be used when talking with the AI." />
        </Div>
        <Div className="vbox" style={{ gap: 4 }}>
          <Input text={timezoneText} onChanged={setTimezoneText} placeholder="UTC offset" />
          <Label variant="secondary" text="Your timezone, detected automatically — change it if it's wrong." />
        </Div>
        <ThemePreview />
        <Div className="vbox" style={{ gap: 4 }}>
          <Div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
            <Label text="Receive notifications when a message is ready" />
            <ToggleSwitch toggled={notificationsEnabled} onToggled={onToggleNotifications} />
          </Div>
        </Div>
        <Div style={{ display: 'flex', gap: 8 }}>
          <Button variant="secondary" text="Back" onClicked={onBack} style={{ width: '50%' }} />
          <Button text="Save" onClicked={onSave} style={{ width: '50%' }} />
        </Div>
      </Div>
    </Div>
  )
}

export default Settings
