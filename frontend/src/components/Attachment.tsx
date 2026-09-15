import { useEffect, useState } from 'react'

import { getFileDownloadUrl } from '../api/files/download'
import { getFile } from '../api/files/get'
import type { FileOut } from '../api/files/types'
import { getFileExtension } from '../utils/fileExtension'
import { AttachmentPreview } from './AttachmentPreview'
import { Button, Div, Label } from './primitives'
import { getMediaKind } from './previewers/registry'
import { WindowsPopup } from './WindowsPopup'

export type AttachmentProps = {
  /** Thumbnail size in px, square. */
  size?: number
  /** Present → renders a small × overlay on the thumbnail that calls this instead of
   * opening the preview. Absent → no remove affordance (e.g. a message already sent). */
  onRemove?: () => void
} & (
  | { kind: 'image'; image: string }
  | {
      kind: 'file'
      /** Just the id — this component fetches the file's own metadata (name, etc.)
       * itself rather than requiring a caller to already have it, so a file attached
       * to a message loaded from history (which only ever carries ids, not full
       * records — see `MessageOut.file_ids`) works the same way as one just
       * uploaded from the composer. */
      fileId: number
    }
)

const DEFAULT_SIZE = 56

/** Initial preview-popup size for a `file`-kind attachment — content with no natural
 * size of its own (a PDF/webpage/document/table, unlike an image) starts at a roomy,
 * sensible default rather than whatever the "Loading…"/"Unknown file" placeholder
 * happened to be, or (for the iframe-based previewers especially) collapsing to
 * nothing: an iframe's `height: 100%` needs a parent with a *real* height to resolve
 * against, which this is what actually provides before the user ever manually resizes. */
const DEFAULT_FILE_PREVIEW_SIZE = { width: 720, height: 520 }

/** Browsers sniff the actual bytes for a raster <img> regardless of the data URL's
 * declared MIME type, so a fixed "image/png" here renders png/jpeg/webp/gif all
 * correctly either way. SVG is the one exception: it's XML text, not a binary format
 * browsers sniff the same way, so a wrong declared type makes it silently fail to
 * render at all rather than just guess right anyway — detected here by decoding just
 * enough of the base64 prefix to check for its telltale opening tag, rather than
 * decoding the whole (possibly large) image just to find this out. */
function toDataUrl(image: string): string {
  const mimeType = looksLikeSvg(image) ? 'image/svg+xml' : 'image/png'
  return `data:${mimeType};base64,${image}`
}

function looksLikeSvg(base64: string): boolean {
  try {
    return /<\s*(\?xml|svg)/i.test(atob(base64.slice(0, 200)))
  } catch {
    return false
  }
}

/** Programmatically triggers a browser download of `href` as `filename` — the
 * click-a-hidden-<a download> trick. The only way to save an in-memory image (base64,
 * never uploaded to the backend) without the user manually right-click-saving it;
 * used for a `file` attachment too for the same one-button experience, even though
 * that case could also just be a plain link (the backend already sets
 * `Content-Disposition: attachment` on it either way). */
function triggerDownload(href: string, filename: string): void {
  const a = document.createElement('a')
  a.href = href
  a.download = filename
  document.body.appendChild(a)
  a.click()
  document.body.removeChild(a)
}

const FILE_ICON = (
  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
    <path d="M14 2v6h6" />
  </svg>
)

/** One attachment, however it's used: a small removable thumbnail while composing, or
 * a plain one attached to an already-sent message — clicking either opens a preview
 * window (the image itself, or a file's content/an "unknown file" notice — see
 * `AttachmentPreview`), closable via its title bar's ×, with a download button right
 * next to it either way. `kind: 'image'` is the original base64-inline attachment
 * (never touches the backend, requires a vision-capable model to actually be read by
 * it); `kind: 'file'` is anything else, already uploaded via `api/files` and
 * referenced by id — see `UserInput`, which decides which kind a picked file becomes. */
export function Attachment(props: AttachmentProps) {
  const { size = DEFAULT_SIZE, onRemove } = props
  const [previewOpen, setPreviewOpen] = useState(false)
  const [file, setFile] = useState<FileOut | null>(null)

  useEffect(() => {
    if (props.kind !== 'file') return
    let cancelled = false
    setFile(null)

    getFile(props.fileId).then((result) => {
      if (!cancelled) setFile(result)
    })

    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.kind === 'file' ? props.fileId : null])

  const download = () => {
    if (props.kind === 'image') triggerDownload(toDataUrl(props.image), 'image.png')
    else if (file) triggerDownload(getFileDownloadUrl(file.id), file.file_name)
  }

  // For a `file`-kind attachment that's actually an image/video — the small chip
  // shows the real thing, same as an inline `kind: 'image'` attachment already does,
  // instead of always falling back to a generic file icon just because the content
  // happens to be referenced by id rather than inlined as base64.
  const mediaKind = props.kind === 'file' && file ? getMediaKind(getFileExtension(file.file_name)) : null
  const thumbnailStyle = {
    width: '100%',
    height: '100%',
    objectFit: 'cover' as const,
    borderRadius: 8,
    border: '1px solid var(--color-border)',
    cursor: 'zoom-in',
  }

  return (
    <>
      <Div style={{ position: 'relative', width: size, height: size }}>
        {props.kind === 'image' ? (
          <img src={toDataUrl(props.image)} alt="" onClick={() => setPreviewOpen(true)} style={thumbnailStyle} />
        ) : mediaKind === 'image' && file ? (
          <img src={getFileDownloadUrl(file.id)} alt={file.file_name} onClick={() => setPreviewOpen(true)} style={thumbnailStyle} />
        ) : mediaKind === 'video' && file ? (
          <video
            src={getFileDownloadUrl(file.id)}
            muted
            preload="metadata"
            onClick={() => setPreviewOpen(true)}
            style={{ ...thumbnailStyle, cursor: 'pointer', background: 'black' }}
          />
        ) : (
          <Div
            className="vbox center"
            onClick={() => setPreviewOpen(true)}
            variant="secondary"
            style={{
              width: '100%',
              height: '100%',
              boxSizing: 'border-box',
              gap: 4,
              padding: 4,
              borderRadius: 8,
              border: '1px solid var(--color-border)',
              cursor: 'pointer',
            }}
          >
            {FILE_ICON}
            <Label
              text={file?.file_name ?? '...'}
              style={{
                fontSize: 9,
                lineHeight: 1.2,
                textAlign: 'center',
                wordBreak: 'break-all',
                display: '-webkit-box',
                WebkitLineClamp: 2,
                WebkitBoxOrient: 'vertical',
                overflow: 'hidden',
              }}
            />
          </Div>
        )}
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
      <WindowsPopup
        open={previewOpen}
        onClose={() => setPreviewOpen(false)}
        title={props.kind === 'image' ? 'Preview' : file?.file_name ?? 'File'}
        onDownload={props.kind === 'image' || file ? download : undefined}
        defaultSize={props.kind === 'file' ? DEFAULT_FILE_PREVIEW_SIZE : undefined}
      >
        {props.kind === 'image' ? (
          // `width`/`height: 100%` only actually constrains anything once the popup's
          // content pane has a real tracked size (i.e. after the user's first resize
          // — see `WindowsPopup`'s own `size` state): before that, a percentage
          // against an auto-sized ancestor resolves as `auto`, so the image still
          // shows at its own natural size same as always. `objectFit: 'contain'` is
          // what makes resizing afterward scale the image to fill the box without
          // stretching it — it's letterboxed instead, same as `image-fit` in a real
          // photo viewer.
          <img
            src={toDataUrl(props.image)}
            alt=""
            style={{ display: 'block', width: '100%', height: '100%', maxWidth: '90vw', maxHeight: '90vh', objectFit: 'contain' }}
          />
        ) : file ? (
          <AttachmentPreview file={file} />
        ) : (
          <Div style={{ padding: 20 }}>
            <Label variant="secondary" text="Loading…" />
          </Div>
        )}
      </WindowsPopup>
    </>
  )
}
