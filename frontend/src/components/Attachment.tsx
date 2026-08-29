import { useState } from 'react'

import { Button, Div } from './primitives'
import { WindowsPopup } from './WindowsPopup'

export interface AttachmentProps {
  /** Base64-encoded image data (no data-URL prefix) — the only attachment kind so far.
   * A future non-image kind would branch on its own shape inside this component, so
   * callers never need to know what an attachment actually is, just how to lay one out. */
  image: string
  /** Thumbnail size in px, square. */
  size?: number
  /** Present → renders a small × overlay on the thumbnail that calls this instead of
   * opening the preview. Absent → no remove affordance (e.g. a message already sent). */
  onRemove?: () => void
}

const DEFAULT_SIZE = 56

/** The exact original format (png/jpeg/webp/...) isn't stored — browsers sniff the
 * actual bytes for an <img>, so a fixed declared type here still renders correctly
 * regardless of what the image actually was. */
function toDataUrl(image: string): string {
  return `data:image/png;base64,${image}`
}

/** One attachment, however it's used: a small removable thumbnail while composing, or a
 * plain one attached to an already-sent message — clicking either opens a full-size,
 * draggable preview window, closable via its title bar's ×. */
export function Attachment({ image, size = DEFAULT_SIZE, onRemove }: AttachmentProps) {
  const [previewOpen, setPreviewOpen] = useState(false)
  const src = toDataUrl(image)

  return (
    <>
      <Div style={{ position: 'relative', width: size, height: size }}>
        <img
          src={src}
          alt=""
          onClick={() => setPreviewOpen(true)}
          style={{
            width: '100%',
            height: '100%',
            objectFit: 'cover',
            borderRadius: 8,
            border: '1px solid var(--color-border)',
            cursor: 'zoom-in',
          }}
        />
        {onRemove ? (
          <Button
            text="×"
            onClicked={onRemove}
            style={{
              position: 'absolute',
              top: -6,
              right: -6,
              width: 18,
              height: 18,
              padding: 0,
              borderRadius: '50%',
              fontSize: 12,
              lineHeight: 1,
            }}
          />
        ) : null}
      </Div>
      <WindowsPopup open={previewOpen} onClose={() => setPreviewOpen(false)} title="Preview">
        <img src={src} alt="" style={{ display: 'block', maxWidth: '90vw', maxHeight: '90vh' }} />
      </WindowsPopup>
    </>
  )
}
