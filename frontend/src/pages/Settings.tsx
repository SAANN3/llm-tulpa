import { useState } from 'react'
import { useNavigate } from 'react-router-dom'

import { Button, Div } from '../components/primitives'
import { AutoConfirmField, NameTimezoneFields, NotificationsField } from '../components/SettingsFields'
import { ThemePreview } from '../components/ThemePreview'
import { TypewriterLabel } from '../components/TypewriterLabel'
import { useSettings } from '../context/useSettings'
import { useDocumentTitle } from '../hooks/useDocumentTitle'
import { getAutoConfirm, setAutoConfirm } from '../utils/autoConfirm'
import { requestNotificationPermission } from '../utils/notifications'
import { validateTimezone } from '../utils/validateTimezone'

/** The browser's local UTC offset in whole hours, used to prefill the timezone field. */
function browserTimezoneOffsetHours(): number {
  return -new Date().getTimezoneOffset() / 60
}

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
  // Local-to-this-browser, unlike everything else on this page — takes effect
  // immediately on toggle rather than waiting for "Save" (see `onToggleAutoConfirm`),
  // since it isn't part of the `Settings` object `onSave` below persists.
  const [autoConfirmEnabled, setAutoConfirmEnabled] = useState(getAutoConfirm)

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

  const onToggleAutoConfirm = (enabled: boolean) => {
    setAutoConfirm(enabled)
    setAutoConfirmEnabled(enabled)
  }

  const onBack = () => navigate('/')

  const tz = validateTimezone(timezoneText)
  const nameValid = name.trim().length > 0
  const saveDisabled = !nameValid || !tz.valid

  const onSave = async () => {
    if (saveDisabled) return

    const timezone = Number(timezoneText)
    await setSettings({ name: name.trim(), timezone, notifications_enabled: notificationsEnabled })
    navigate('/')
  }

  return (
    <Div className="page center vbox" style={{ gap: 16 }}>
      <TypewriterLabel className="mono" text='[ Settings ]' charIntervalMs={30} style={{ fontSize: 15, letterSpacing: '0.14em' }} />
      <Div
        className="vbox"
        style={{
          width: 440,
          padding: 26,
          gap: 24,
          borderRadius: 14,
          border: '1px solid var(--color-border)',
          background: 'var(--color-surface)',
        }}
      >
        <NameTimezoneFields name={name} onNameChanged={setName} timezoneText={timezoneText} onTimezoneChanged={setTimezoneText} />
        <ThemePreview />
        <NotificationsField enabled={notificationsEnabled} onToggle={onToggleNotifications} />
        <AutoConfirmField enabled={autoConfirmEnabled} onToggle={onToggleAutoConfirm} />
        <Div style={{ display: 'flex', gap: 8 }}>
          <Button variant="secondary" text="Back" onClicked={onBack} style={{ flex: 1 }} />
          <Button text="Save" onClicked={onSave} disabled={saveDisabled} style={{ flex: 1 }} />
        </Div>
      </Div>
    </Div>
  )
}

export default Settings
