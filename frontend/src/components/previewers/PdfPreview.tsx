import axios from 'axios'
import { useEffect, useState } from 'react'

import { getFileDownloadUrl } from '../../api/files/download'
import { Div, Label } from '../primitives'
import type { PreviewerProps } from './types'

/** Renders via the browser's own built-in PDF viewer, inside an iframe pointed at a
 * `blob:` URL — not the download URL directly: that route sets `Content-Disposition:
 * attachment`, which makes a navigation (which is what pointing an iframe at a URL
 * does) download the file instead of rendering it. A `blob:` URL isn't a network
 * request at all, so that header never comes into play. */
export default function PdfPreview({ file }: PreviewerProps) {
  const [blobUrl, setBlobUrl] = useState<string | null>(null)
  const [failed, setFailed] = useState(false)

  useEffect(() => {
    let cancelled = false
    let url: string | null = null

    axios
      .get<Blob>(getFileDownloadUrl(file.id), { responseType: 'blob' })
      .then((res) => {
        if (cancelled) return
        url = URL.createObjectURL(res.data)
        setBlobUrl(url)
      })
      .catch(() => {
        if (!cancelled) setFailed(true)
      })

    return () => {
      cancelled = true
      if (url) URL.revokeObjectURL(url)
    }
  }, [file.id])

  if (failed) {
    return (
      <Div style={{ padding: 20 }}>
        <Label text="Couldn't load this file." />
      </Div>
    )
  }

  if (!blobUrl) {
    return (
      <Div style={{ padding: 20 }}>
        <Label variant="secondary" text="Loading…" />
      </Div>
    )
  }

  return <iframe src={blobUrl} title={file.file_name} style={{ width: '100%', height: '100%', border: 'none', display: 'block' }} />
}
