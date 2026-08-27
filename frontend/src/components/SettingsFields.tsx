import { Div, Input, Label, ToggleSwitch } from './primitives'
import { validateTimezone } from '../utils/validateTimezone'

/** A field label in the small-caps style used above every input in this panel. */
function FieldLabel({ text }: { text: string }) {
  return <Label className="section-heading" text={text} style={{ fontSize: 11, letterSpacing: '0.09em' }} />
}

/** Helper/status text under a field. */
function FieldHelp({ text, accent, maxWidth }: { text: string; accent?: boolean; maxWidth?: string }) {
  return (
    <Label
      variant="secondary"
      text={text}
      style={{
        fontSize: 12.5,
        lineHeight: 1.45,
        opacity: accent ? 1 : 0.6,
        color: accent ? 'var(--color-tertiary)' : undefined,
        maxWidth,
      }}
    />
  )
}

export interface NameTimezoneFieldsProps {
  name: string
  onNameChanged: (name: string) => void
  timezoneText: string
  onTimezoneChanged: (text: string) => void
}

/** Name + timezone fields — Settings step 1 / Setup step 1. */
export function NameTimezoneFields({ name, onNameChanged, timezoneText, onTimezoneChanged }: NameTimezoneFieldsProps) {
  const tz = validateTimezone(timezoneText)

  return (
    <Div className="vbox" style={{ gap: 20 }}>
      <Div className="vbox" style={{ gap: 4 }}>
        <FieldLabel text="Name" />
        <Input
          text={name}
          onChanged={onNameChanged}
          placeholder="Enter your name"
          style={{ borderRadius: 7, padding: '9px 11px' }}
        />
        <FieldHelp text="This name will be used when talking with the AI." />
      </Div>
      <Div className="vbox" style={{ gap: 4 }}>
        <FieldLabel text="Timezone" />
        <Div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <Input
            text={timezoneText}
            onChanged={onTimezoneChanged}
            placeholder="UTC offset"
            style={{ width: 110, borderRadius: 7, padding: '9px 11px' }}
          />
          {tz.echo ? <Label className="mono" variant="secondary" text={tz.echo} style={{ fontSize: 13 }} /> : null}
        </Div>
        <FieldHelp text={tz.message} accent={!tz.valid} />
      </Div>
    </Div>
  )
}

export interface NotificationsFieldProps {
  enabled: boolean
  onToggle: (enabled: boolean) => void
}

/** Notifications toggle — Settings step 3 / Setup step 3. */
export function NotificationsField({ enabled, onToggle }: NotificationsFieldProps) {
  return (
    <Div className="vbox" style={{ gap: 4 }}>
      <Div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
        <Label text="Receive notifications when a message is ready" style={{ fontSize: 15, maxWidth: '32ch' }} />
        <ToggleSwitch toggled={enabled} onToggled={onToggle} />
      </Div>
      <FieldHelp text="Asks your browser for permission — you can change this later in Settings." maxWidth="40ch" />
    </Div>
  )
}
