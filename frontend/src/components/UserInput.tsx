import type { ChangeEvent, CSSProperties, DragEvent } from 'react'
import { useEffect, useRef, useState } from 'react'

import { uploadFile } from '../api/files/upload'
import { Attachment } from './Attachment'
import { Button, Div, Label, TextField, ToggleSwitch } from './primitives'

export interface UserInputProps {
  text?: string
  blocked: boolean
  /** `images` are base64-encoded (no data-URL prefix), one entry per attached image.
   * `fileIds` are ids of non-image files already uploaded while composing (see
   * `chatId`). */
  onSended: (text: string, think: boolean, images: string[], fileIds: number[]) => void
  style?: CSSProperties
  /** Placeholder shown in the empty textarea — passed in rather than hardcoded so different pages (or a future generated prompt) can supply their own. */
  placeholder?: string
  /** Whether the draft is cleared right after `onSended` fires. Defaults to true (normal chat behavior); set false when the page is about to navigate away and wants the sent text to stay visible (just disabled, via `blocked`) instead of flashing empty first. */
  clearOnSend?: boolean
  /** Whether the textarea itself is disabled. Defaults to `blocked` (the send button's own disabled state) when not given — e.g. Home, where creating a chat should block typing too. Pass `false` explicitly to keep the textarea typable while only the send button is blocked, e.g. Chat, where the composer should stay usable while a previous message is still in flight. */
  inputDisabled?: boolean
  /** Starting state of the "Thinking" toggle. Defaults to true. Chat seeds this from a pending prompt's own `think` value (set on Home when the message was sent) so the toggle shown matches what's actually about to be requested, instead of visually resetting to the default the moment the new chat page mounts. */
  initialThink?: boolean
  /** The chat a picked non-image file gets uploaded against, if there is one yet.
   * Omit on a page with no chat yet (e.g. Home, before the first message that'll
   * create one has been sent) — the file still uploads, just with no `chat_id` (see
   * `uploadFile`); it gets claimed automatically once it's actually attached to a
   * sent message. */
  chatId?: number
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

function isImageFile(file: File): boolean {
  return file.type.startsWith('image/')
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
  chatId,
}: UserInputProps) {
  const [value, setValue] = useState(text ?? '')
  const [think, setThink] = useState(initialThink)
  const [images, setImages] = useState<string[]>([])
  const [fileIds, setFileIds] = useState<number[]>([])
  const [uploading, setUploading] = useState(false)
  const [draggingOver, setDraggingOver] = useState(false)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  // Counts nested drag-enter/leave pairs rather than toggling straight off `onDragLeave`
  // — a leave fires every time the cursor crosses from the container onto one of its
  // own children (the textarea, the attachment chips, ...), which would otherwise flip
  // `draggingOver` off and back on repeatedly while dragging around inside the same
  // drop zone. Only actually leaving the outermost container brings this back to 0.
  const dragDepthRef = useRef(0)

  useEffect(() => {
    const el = textareaRef.current
    if (!el) return

    el.style.height = 'auto'
    el.style.height = `${el.scrollHeight}px`
  }, [value])

  const canSend = (value.trim().length > 0 || images.length > 0 || fileIds.length > 0) && !uploading

  const send = () => {
    if (blocked || !canSend) return
    onSended(value, think, images, fileIds)
    if (clearOnSend) {
      setValue('')
      setImages([])
      setFileIds([])
    }
  }

  /** Shared by the hidden file-picker input and drag-and-drop — either way a caller
   * just hands over whatever `File`s the browser gave it. */
  const addFiles = async (files: File[]) => {
    const imageFiles = files.filter(isImageFile)
    const otherFiles = files.filter((file) => !isImageFile(file))

    if (imageFiles.length > 0) {
      const encoded = await Promise.all(imageFiles.map(readFileAsBase64))
      setImages((prev) => [...prev, ...encoded])
    }

    if (otherFiles.length > 0) {
      setUploading(true)
      try {
        const uploaded = await Promise.all(otherFiles.map((file) => uploadFile(file, chatId)))
        setFileIds((prev) => [...prev, ...uploaded.map((file) => file.id)])
      } finally {
        setUploading(false)
      }
    }
  }

  const onFilePicked = (e: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files ?? [])
    e.target.value = '' // lets picking the same file again re-trigger onChange
    void addFiles(files)
  }

  const inputIsDisabled = inputDisabled ?? blocked
  const dropDisabled = inputIsDisabled || uploading

  // `dataTransfer.files` is only actually populated by the `drop` event itself —
  // `dragover`/`dragenter` only ever expose `dataTransfer.types`, so `types.includes
  // ('Files')` is as far as "is this even a file drag" can be told before the drop
  // happens (lets a plain text/link drag pass through without hijacking it or
  // showing the drop-zone highlight for something that was never going to add a
  // file anyway).
  const isFileDrag = (e: DragEvent<HTMLDivElement>) => e.dataTransfer.types.includes('Files')

  const onDragEnter = (e: DragEvent<HTMLDivElement>) => {
    if (dropDisabled || !isFileDrag(e)) return
    e.preventDefault()
    dragDepthRef.current += 1
    setDraggingOver(true)
  }

  const onDragOver = (e: DragEvent<HTMLDivElement>) => {
    if (dropDisabled || !isFileDrag(e)) return
    e.preventDefault() // required for `onDrop` to ever fire at all
  }

  const onDragLeave = (e: DragEvent<HTMLDivElement>) => {
    if (dropDisabled || !isFileDrag(e)) return
    e.preventDefault()
    dragDepthRef.current = Math.max(0, dragDepthRef.current - 1)
    if (dragDepthRef.current === 0) setDraggingOver(false)
  }

  const onDrop = (e: DragEvent<HTMLDivElement>) => {
    if (dropDisabled) return
    e.preventDefault()
    dragDepthRef.current = 0
    setDraggingOver(false)
    const files = Array.from(e.dataTransfer.files)
    if (files.length > 0) void addFiles(files)
  }

  const removeImage = (index: number) => setImages((prev) => prev.filter((_, i) => i !== index))
  const removeFileId = (id: number) => setFileIds((prev) => prev.filter((fileId) => fileId !== id))

  return (
    <Div
      onDragEnter={onDragEnter}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
      style={{
        position: 'relative',
        display: 'flex',
        flexDirection: 'column',
        gap: 8,
        width: '100%',
        boxSizing: 'border-box',
        border: `1px ${draggingOver ? 'dashed' : 'solid'} var(--color-border)`,
        borderColor: draggingOver ? 'var(--color-primary)' : 'var(--color-border)',
        borderRadius: 12,
        background: 'var(--color-surface)',
        padding: 8,
        ...style,
      }}
    >
      {draggingOver ? (
        <Div
          className="vbox center"
          style={{
            position: 'absolute',
            inset: 4,
            borderRadius: 8,
            background: 'var(--color-surface)',
            opacity: 0.92,
            zIndex: 1,
            pointerEvents: 'none',
          }}
        >
          <Label text="Drop to attach" style={{ fontSize: 13, fontWeight: 600, opacity: 0.8 }} />
        </Div>
      ) : null}
      {images.length > 0 || fileIds.length > 0 ? (
        <Div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          {images.map((image, index) => (
            <Attachment key={`image-${index}`} kind="image" image={image} onRemove={() => removeImage(index)} />
          ))}
          {fileIds.map((id) => (
            <Attachment key={`file-${id}`} kind="file" fileId={id} onRemove={() => removeFileId(id)} />
          ))}
        </Div>
      ) : null}
      <input ref={fileInputRef} type="file" multiple onChange={onFilePicked} style={{ display: 'none' }} />
      <TextField
        ref={textareaRef}
        text={value}
        onChanged={setValue}
        placeholder={placeholder}
        disabled={inputIsDisabled}
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
          disabled={dropDisabled}
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
