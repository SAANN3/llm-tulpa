import { useEffect, useState } from 'react'
import type { CSSProperties } from 'react'

import type { PluginInfo, PropertyInfo, PropertyType } from '../api/plugins/types'
import { Button, Div, Input, Label, TextField, ToggleSwitch } from './primitives'

export interface PluginSettingsProps {
  plugin: PluginInfo
  getSchema: (pluginName: string, pluginSubname: string) => Promise<PropertyInfo[]>
  getHelp: (pluginName: string, pluginSubname: string) => Promise<string>
  onSave: (settings: Record<string, unknown>) => Promise<void>
  onBack: () => void
}

/** `array` fields get their own row per item (an input + a remove button, plus an "add
 * row" button) rather than one packed text field — there's no per-element schema (e.g.
 * an array's item type) to build a dedicated widget from, but every array this schema
 * system actually describes so far is a list of strings, and a row-per-value editor
 * makes each entry (and the ability to add/remove one) hard to miss, unlike a single
 * comma-separated input. `object` has no equivalent shorthand, so it stays raw JSON. */
function cleanList(items: string[]): string[] {
  return items.map((item) => item.trim()).filter((item) => item.length > 0)
}

function defaultTextFor(type: PropertyType): string {
  return type === 'object' ? '{}' : ''
}

/** The +/− row buttons in an `array` field's editor — a fixed small square rather than
 * the `Button` primitive, which is built for full-width text buttons, not a compact
 * per-row control. */
const rowButtonStyle: CSSProperties = {
  boxSizing: 'border-box',
  width: 28,
  height: 28,
  flexShrink: 0,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  borderRadius: 6,
  border: '1px solid var(--color-border)',
  cursor: 'pointer',
  fontSize: 15,
  lineHeight: 1,
}

/** "<what kind of value> — <a concrete example>", shown above the field's own
 * (backend-authored) description — the schema only says a field's JSON type, which
 * isn't itself enough to know how to fill the widget in, so this spells that out per
 * widget. `null` for `boolean`, which has no text to type — the toggle already says
 * everything. */
function typeHint(type: PropertyType): string | null {
  switch (type) {
    case 'string':
      return 'Text — e.g. my-value'
    case 'number':
      return 'Number — e.g. 3.14'
    case 'integer':
      return 'Integer — e.g. 42'
    case 'array':
      return 'One value per row — e.g. value1'
    case 'object':
      return 'JSON object — e.g. {"key": "value"}'
    case 'boolean':
      return null
  }
}

/** A settings form built from a plugin's own schema (`getSchema`) rather than
 * hand-written per plugin — every field type this schema system can describe
 * (`PropertyType`) gets one generic widget. Prefilled from `plugin.settings` when the
 * plugin already has some; `onSave` receives the whole settings object at once, same
 * shape `POST /api/plugins/settings` expects. */
export function PluginSettings({ plugin, getSchema, getHelp, onSave, onBack }: PluginSettingsProps) {
  const [schema, setSchema] = useState<PropertyInfo[] | null>(null)
  const [help, setHelp] = useState<string | null>(null)
  const [helpExpanded, setHelpExpanded] = useState(false)
  const [textValues, setTextValues] = useState<Record<string, string>>({})
  const [boolValues, setBoolValues] = useState<Record<string, boolean>>({})
  const [arrayValues, setArrayValues] = useState<Record<string, string[]>>({})
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    setSchema(null)
    setHelp(null)
    setHelpExpanded(false)
    setError(null)

    getHelp(plugin.plugin_name, plugin.plugin_subname).then(setHelp)

    getSchema(plugin.plugin_name, plugin.plugin_subname).then((fields) => {
      const current = plugin.settings ?? {}
      const text: Record<string, string> = {}
      const bools: Record<string, boolean> = {}
      const arrays: Record<string, string[]> = {}

      for (const field of fields) {
        const value = current[field.name]
        if (field.property_type === 'boolean') {
          bools[field.name] = typeof value === 'boolean' ? value : false
        } else if (field.property_type === 'array') {
          arrays[field.name] = Array.isArray(value) ? value.map((item) => String(item)) : []
        } else if (field.property_type === 'object') {
          text[field.name] = value === undefined ? defaultTextFor('object') : JSON.stringify(value, null, 2)
        } else {
          text[field.name] = value === undefined ? '' : String(value)
        }
      }

      setTextValues(text)
      setBoolValues(bools)
      setArrayValues(arrays)
      setSchema(fields)
    })
    // Deliberately keyed on the plugin's identity, not the `plugin`/`getSchema`
    // references themselves — both get a new identity on every `usePlugins` re-render
    // (a fresh closure, a fresh mapped array), which would refetch the schema on every
    // unrelated state change instead of only when the user actually navigates to a
    // different plugin.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [plugin.plugin_name, plugin.plugin_subname])

  const onSubmit = async () => {
    if (!schema) return
    setError(null)

    const settings: Record<string, unknown> = {}
    try {
      for (const field of schema) {
        if (field.property_type === 'boolean') {
          settings[field.name] = boolValues[field.name] ?? false
        } else if (field.property_type === 'number' || field.property_type === 'integer') {
          const raw = textValues[field.name] ?? ''
          const num = Number(raw)
          if (raw.trim() === '' || Number.isNaN(num)) throw new Error(`"${field.name}" must be a number.`)
          settings[field.name] = num
        } else if (field.property_type === 'array') {
          const list = cleanList(arrayValues[field.name] ?? [])
          if (field.required && list.length === 0) throw new Error(`"${field.name}" is required.`)
          settings[field.name] = list
        } else if (field.property_type === 'object') {
          const raw = textValues[field.name] || defaultTextFor('object')
          let parsed: unknown
          try {
            parsed = JSON.parse(raw)
          } catch {
            throw new Error(`"${field.name}" must be valid JSON.`)
          }
          if (Array.isArray(parsed) || typeof parsed !== 'object' || parsed === null) {
            throw new Error(`"${field.name}" must be a JSON object, e.g. {}.`)
          }
          settings[field.name] = parsed
        } else {
          // An empty required string is treated the same as never having been filled
          // in — `''` is a valid *value* for an optional field, but for a required one
          // it means nothing was actually entered, so it's caught here rather than
          // silently sent as an empty string.
          const raw = textValues[field.name] ?? ''
          if (field.required && raw.trim() === '') throw new Error(`"${field.name}" is required.`)
          settings[field.name] = raw
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Some fields are invalid.')
      return
    }

    setSaving(true)
    try {
      await onSave(settings)
      onBack()
    } catch {
      setError('Failed to save settings.')
      setSaving(false)
    }
  }

  const addArrayItem = (fieldName: string) =>
    setArrayValues((prev) => ({ ...prev, [fieldName]: [...(prev[fieldName] ?? []), ''] }))

  const updateArrayItem = (fieldName: string, index: number, value: string) =>
    setArrayValues((prev) => ({
      ...prev,
      [fieldName]: (prev[fieldName] ?? []).map((item, i) => (i === index ? value : item)),
    }))

  const removeArrayItem = (fieldName: string, index: number) =>
    setArrayValues((prev) => ({ ...prev, [fieldName]: (prev[fieldName] ?? []).filter((_, i) => i !== index) }))

  return (
    <Div className="vbox" style={{ gap: 16 }}>
      <Label text={`${plugin.plugin_subname} settings`} style={{ fontSize: 15 }} />

      {/* One scroll region for everything below the heading — help card included — so a
          long help message and a long field list scroll together instead of the help
          card pushing the (separately scrollable) field list off screen. */}
      <Div className="vbox" style={{ gap: 16, maxHeight: '50vh', overflowY: 'auto' }}>
        {help ? (
          <Div
            className="vbox"
            style={{
              gap: 4,
              padding: '10px 12px',
              borderRadius: 8,
              border: '1px solid var(--color-border)',
              background: 'var(--color-background)',
            }}
          >
            <Div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 8 }}>
              <Label className="section-heading" text="How to use" style={{ fontSize: 11, letterSpacing: '0.09em' }} />
              <Button
                variant="secondary"
                text={helpExpanded ? 'Hide' : 'Show'}
                onClicked={() => setHelpExpanded((prev) => !prev)}
                style={{ padding: '3px 10px', fontSize: 11.5, width: 'auto' }}
              />
            </Div>
            {helpExpanded ? (
              <Label
                variant="secondary"
                text={help}
                style={{ fontSize: 12.5, lineHeight: 1.5, opacity: 0.75, whiteSpace: 'pre-wrap' }}
              />
            ) : null}
          </Div>
        ) : null}

        {schema === null ? (
          <Label variant="secondary" text="Loading…" />
        ) : (
          schema.map((field) => (
            <Div className="vbox" key={field.name} style={{ gap: 4 }}>
              <Div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <Label className="section-heading" text={field.name} style={{ fontSize: 11, letterSpacing: '0.09em' }} />
                {field.property_type === 'boolean' ? (
                  <ToggleSwitch
                    toggled={boolValues[field.name] ?? false}
                    onToggled={(toggled) => setBoolValues((prev) => ({ ...prev, [field.name]: toggled }))}
                  />
                ) : null}
              </Div>
              {field.property_type === 'boolean' ? null : field.property_type === 'object' ? (
                <TextField
                  text={textValues[field.name] ?? ''}
                  onChanged={(text) => setTextValues((prev) => ({ ...prev, [field.name]: text }))}
                  style={{ borderRadius: 7, padding: '9px 11px', minHeight: 70, fontFamily: 'ui-monospace, monospace', fontSize: 13 }}
                />
              ) : field.property_type === 'array' ? (
                <Div className="vbox" style={{ gap: 6 }}>
                  {(arrayValues[field.name] ?? []).map((item, index) => (
                    // eslint-disable-next-line react/no-array-index-key -- rows have no other stable id, and are always read/written by this same index anyway
                    <Div key={index} style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
                      <Input
                        text={item}
                        onChanged={(text) => updateArrayItem(field.name, index, text)}
                        style={{ flex: 1, borderRadius: 7, padding: '9px 11px' }}
                      />
                      <Div style={rowButtonStyle} onClick={() => removeArrayItem(field.name, index)}>
                        <Label text="−" />
                      </Div>
                    </Div>
                  ))}
                  <Div style={{ ...rowButtonStyle, width: '100%' }} onClick={() => addArrayItem(field.name)}>
                    <Label text="+" />
                  </Div>
                </Div>
              ) : (
                <Input
                  text={textValues[field.name] ?? ''}
                  onChanged={(text) => setTextValues((prev) => ({ ...prev, [field.name]: text }))}
                  style={{ borderRadius: 7, padding: '9px 11px' }}
                />
              )}
              {typeHint(field.property_type) ? (
                <Label className="mono" variant="secondary" text={typeHint(field.property_type)!} style={{ fontSize: 11.5, opacity: 0.55 }} />
              ) : null}
              <Label variant="secondary" text={field.description} style={{ fontSize: 12.5, lineHeight: 1.45, opacity: 0.6 }} />
            </Div>
          ))
        )}
      </Div>

      {error ? <Label text={error} style={{ fontSize: 12.5, color: 'var(--color-tertiary)' }} /> : null}

      <Div style={{ display: 'flex', gap: 8 }}>
        <Button variant="secondary" text="Cancel" onClicked={onBack} style={{ flex: 1 }} />
        <Button text="Save" onClicked={onSubmit} disabled={schema === null || saving} style={{ flex: 1 }} />
      </Div>
    </Div>
  )
}
