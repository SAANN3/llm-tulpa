import axios from 'axios'
import { useEffect, useState } from 'react'

import { getFileDownloadUrl } from '../../api/files/download'
import type { FileOut } from '../../api/files/types'

/** Fetches `file`'s content as plain text — shared by every text-based previewer
 * (`TextPreview`, `CodePreview`, `HtmlPreview`). `Content-Disposition: attachment` on
 * the download route only matters for a real browser navigation (it's what makes
 * clicking the link save instead of render); a programmatic request like this one
 * gets the body back either way. */
export function useTextContent(file: FileOut): { content: string | null; failed: boolean } {
  const [content, setContent] = useState<string | null>(null)
  const [failed, setFailed] = useState(false)

  useEffect(() => {
    let cancelled = false
    setContent(null)
    setFailed(false)

    axios
      .get<string>(getFileDownloadUrl(file.id), { responseType: 'text' })
      .then((res) => {
        if (!cancelled) setContent(res.data)
      })
      .catch(() => {
        if (!cancelled) setFailed(true)
      })

    return () => {
      cancelled = true
    }
  }, [file.id])

  return { content, failed }
}
