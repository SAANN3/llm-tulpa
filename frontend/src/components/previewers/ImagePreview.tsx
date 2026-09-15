import { useState } from 'react'

import { getFileDownloadUrl } from '../../api/files/download'
import { Div, Label } from '../primitives'
import type { PreviewerProps } from './types'

/** A plain `<img>` pointed straight at the download URL — unlike `PdfPreview`'s iframe,
 * an embedded media element loads a resource to render, not to navigate, so
 * `Content-Disposition: attachment` on that route never causes a download here the
 * way it would pointing an iframe (or a top-level link) at the same URL. `objectFit:
 * 'contain'` keeps the image's own aspect ratio as the popup gets resized, matching
 * how an inline (base64) image attachment already behaves. */
export default function ImagePreview({ file }: PreviewerProps) {
  const [failed, setFailed] = useState(false)

  if (failed) {
    return (
      <Div style={{ padding: 20 }}>
        <Label text="Couldn't load this image." />
      </Div>
    )
  }

  return (
    <img
      src={getFileDownloadUrl(file.id)}
      alt={file.file_name}
      onError={() => setFailed(true)}
      style={{ width: '100%', height: '100%', objectFit: 'contain', display: 'block' }}
    />
  )
}
