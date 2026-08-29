import type { ChangeEvent, CSSProperties } from 'react'
import { useEffect, useRef, useState } from 'react'

import { Attachment } from './Attachment'
import { Button, Div, Label, TextField, ToggleSwitch } from './primitives'

export interface UserInputProps {
  text?: string
  blocked: boolean
  /** `images` are base64-encoded (no data-URL prefix), one entry per attached image. */
  onSended: (text: string, think: boolean, images: string[]) => void
  style?: CSSProperties
  /** Placeholder shown in the empty textarea — passed in rather than hardcoded so different pages (or a future generated prompt) can supply their own. */
  placeholder?: string
  /** Whether the draft is cleared right after `onSended` fires. Defaults to true (normal chat behavior); set false when the page is about to navigate away and wants the sent text to stay visible (just disabled, via `blocked`) instead of flashing empty first. */
  clearOnSend?: boolean
  /** Whether the textarea itself is disabled. Defaults to `blocked` (the send button's own disabled state) when not given — e.g. Home, where creating a chat should block typing too. Pass `false` explicitly to keep the textarea typable while only the send button is blocked, e.g. Chat, where the composer should stay usable while a previous message is still in flight. */
  inputDisabled?: boolean
  /** Starting state of the "Thinking" toggle. Defaults to true. Chat seeds this from a pending prompt's own `think` value (set on Home when the message was sent) so the toggle shown matches what's actually about to be requested, instead of visually resetting to the default the moment the new chat page mounts. */
  initialThink?: boolean
}

/** How many lines tall the textarea is allowed to grow before it caps and scrolls internally instead — 1 starting line plus this many more. */
const MAX_EXTRA_LINES = 3

const DEFAULT_PLACEHOLDER = 'Message...'

/** Reads `file` into raw base64 (no `data:image/...;base64,` prefix) — the wire format
 * Ollama and the backend's storage both want; `Attachment` reconstructs a data URL from
 * this itself for preview, so nothing else here needs to touch the prefixed form. */
function readFileAsBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => {
      const dataUrl = reader.result as string
      resolve(dataUrl.slice(dataUrl.indexOf(',') + 1))
    }
    reader.onerror = () => reject(reader.error)
    reader.readAsDataURL(file)
  })
}

/** The chat composer — holds its own draft text (optionally seeded by `text`), clears it and calls `onSended` on send. `blocked` disables sending while the model is still answering. Enter sends; Shift+Enter inserts a newline. Grows in height as the draft wraps to more lines, up to `MAX_EXTRA_LINES` past the first, then scrolls internally instead of growing further. `style` overrides the outer container's own defaults (e.g. `width`), so callers can size it differently per page. */
export function UserInput({
  text,
  blocked,
  onSended,
  style,
  placeholder = DEFAULT_PLACEHOLDER,
  clearOnSend = true,
  inputDisabled,
  initialThink = true,
}: UserInputProps) {
  const [value, setValue] = useState(text ?? '')
  const [think, setThink] = useState(initialThink)
  const [images, setImages] = useState<string[]>([])
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    const el = textareaRef.current
    if (!el) return

    el.style.height = 'auto'
    el.style.height = `${el.scrollHeight}px`
  }, [value])

  const canSend = value.trim().length > 0 || images.length > 0

  const send = () => {
    if (blocked || !canSend) return
    onSended(value, think, images)
    if (clearOnSend) {
      setValue('')
      setImages([])
    }
  }

  const addFiles = async (e: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files ?? [])
    e.target.value = '' // lets picking the same file again re-trigger onChange
    const encoded = await Promise.all(files.map(readFileAsBase64))
    setImages((prev) => [...prev, ...encoded])
  }

  const removeImage = (index: number) => setImages((prev) => prev.filter((_, i) => i !== index))

  return (
    <Div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: 8,
        width: '100%',
        boxSizing: 'border-box',
        border: '1px solid var(--color-border)',
        borderRadius: 12,
        background: 'var(--color-surface)',
        padding: 8,
        ...style,
      }}
    >
      {images.length > 0 ? (
        <Div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          {images.map((image, index) => (
            <Attachment key={index} image={image} onRemove={() => removeImage(index)} />
          ))}
        </Div>
      ) : null}
      <input
        ref={fileInputRef}
        type="file"
        accept="image/*"
        multiple
        onChange={addFiles}
        style={{ display: 'none' }}
      />
      <TextField
        ref={textareaRef}
        text={value}
        onChanged={setValue}
        placeholder={placeholder}
        disabled={inputDisabled ?? blocked}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault()
            send()
          }
        }}
        style={{
          width: '100%',
          resize: 'none',
          minHeight: 44,
          maxHeight: `calc(${1 + MAX_EXTRA_LINES} * 1.4em + 1em)`,
          overflowY: 'auto',
          border: 'none',
          borderRadius: 8,
          padding: '11px 12px',
        }}
      />
      <Div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <Label variant="secondary" text="Enter to send · Shift+Enter for a new line" style={{ fontSize: 11, opacity: 0.6 }} />
        <Div style={{ flex: 1 }} />
        <Label variant="secondary" text="Thinking" style={{ fontSize: 12, opacity: 0.6 }} />
        <ToggleSwitch toggled={think} onToggled={setThink} disabled={blocked} />
        <Button
          onClicked={() => fileInputRef.current?.click()}
          disabled={inputDisabled ?? blocked}
          style={{ width: 28, height: 28, padding: 0, borderRadius: 6, display: 'flex', alignItems: 'center', justifyContent: 'center' }}
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48" />
          </svg>
        </Button>
        <Button text="Send" onClicked={send} disabled={blocked || !canSend} />
      </Div>
    </Div>
  )
}
