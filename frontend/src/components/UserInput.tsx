import type { ChangeEvent, CSSProperties, DragEvent } from 'react'
import { useEffect, useRef, useState } from 'react'

import type { ThinkChoice } from '../api/agent/types'
import { uploadFile } from '../api/files/upload'
import { getThinkingCapability } from '../api/llm/thinking_capability'
import { Attachment as AttachmentIcon } from 'pixelarticons/react'

import '../styles/UserInput.scss'
import { Attachment } from './Attachment'
import { Button, Div, Label, Select, TextField, ToggleSwitch } from './primitives'

export interface UserInputProps {
  text?: string
  blocked: boolean
  /** Extra class on the outer container — e.g. Home sizes the composer via `home__composer`. */
  className?: string
  /** `images` are base64-encoded (no data-URL prefix), one entry per attached image.
   * `fileIds` are ids of non-image files already uploaded while composing (see
   * `chatId`). */
  onSended: (text: string, think: ThinkChoice, images: string[], fileIds: number[]) => void
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

/** The chat composer — holds its own draft text (optionally seeded by `text`), clears it and calls `onSended` on send. `blocked` disables sending while the model is still answering. Enter sends; Shift+Enter inserts a newline. Grows in height as the draft wraps to more lines, up to a few past the first (capped in `UserInput.scss`), then scrolls internally instead of growing further. `style` overrides the outer container's own defaults (e.g. `width`), so callers can size it differently per page. */
export function UserInput({
  text,
  blocked,
  onSended,
  style,
  className,
  placeholder = DEFAULT_PLACEHOLDER,
  clearOnSend = true,
  inputDisabled,
  initialThink = true,
  chatId,
}: UserInputProps) {
  const [value, setValue] = useState(text ?? '')
  const [think, setThink] = useState(initialThink)
  // Discovered fresh every time this composer mounts — not cached/stored anywhere,
  // by design (the active model can change without this app restarting, and this
  // call is cheap: Ollama's `/api/show` reads stored model metadata, no load
  // required). `null` while loading/unknown; a specific mode is only ever sent on
  // the wire once the user actually picks one (see `send` below) — until then,
  // thinking-on falls back to the model's own default effort, unchanged from before
  // this feature existed.
  const [thinkingModes, setThinkingModes] = useState<string[] | null>(null)
  const [thinkMode, setThinkMode] = useState<string | null>(null)
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

  useEffect(() => {
    let cancelled = false
    getThinkingCapability().then(
      (capability) => {
        if (cancelled) return
        if (capability.kind === 'graduated') {
          setThinkingModes(capability.modes)
          setThinkMode((current) => current ?? capability.modes[0] ?? null)
        } else {
          setThinkingModes(null)
        }
      },
      () => {
        // A model with no thinking control at all, or a transient failure to reach
        // it, both look the same from here: no graduated modes to offer — the
        // existing on/off toggle (which needs no capability info) still works
        // either way.
        if (!cancelled) setThinkingModes(null)
      },
    )
    return () => {
      cancelled = true
    }
  }, [])

  const canSend = (value.trim().length > 0 || images.length > 0 || fileIds.length > 0) && !uploading

  const send = () => {
    if (blocked || !canSend) return
    // `false` (toggle off) always wins. Otherwise: a specific mode if the model
    // supports levels and one's actually selected, else plain `true` — the model's
    // own default effort, exactly what this app sent before this feature existed.
    const thinkChoice: ThinkChoice = !think ? false : thinkMode ?? true
    onSended(value, thinkChoice, images, fileIds)
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
      className={['composer', draggingOver && 'composer--dragging', className].filter(Boolean).join(' ')}
      style={style}
    >
      {draggingOver ? (
        <Div className="vbox center composer__overlay">
          <Label className="composer__overlay-text" text="Drop to attach" />
        </Div>
      ) : null}
      {images.length > 0 || fileIds.length > 0 ? (
        <Div className="composer__attachments">
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
        className="composer__input"
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
      />
      <Div className="composer__footer">
        <Label variant="secondary" className="composer__hint" text="Enter to send · Shift+Enter for a new line" />
        <Div className="composer__spacer" />
        <Label variant="secondary" className="composer__think-label" text="Thinking" />
        <ToggleSwitch toggled={think} onToggled={setThink} disabled={blocked} />
        {thinkingModes ? (
          // `Select` has no native `disabled` prop — blocked visually and
          // functionally (no click-through) via the wrapper instead, rather than
          // adding one just for this single usage. Blocked whenever thinking itself
          // is off, since a mode choice is meaningless without it.
          <Div className={['composer__mode', !think && 'composer__mode--disabled'].filter(Boolean).join(' ')}>
            <Select values={thinkingModes} selected={thinkMode ?? undefined} onChosen={setThinkMode} />
          </Div>
        ) : null}
        <Button className="composer__icon-button" onClicked={() => fileInputRef.current?.click()} disabled={dropDisabled}>
          <AttachmentIcon width={20} height={20} />
        </Button>
        <Button className="composer__send" text="Send" onClicked={send} disabled={blocked || !canSend} />
      </Div>
    </Div>
  )
}
