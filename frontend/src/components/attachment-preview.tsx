import { lazy, Suspense } from 'react'

import type { FileOut } from '../api/files/types'
import { getFileExtension } from '../utils/file-extension.ts'
import { Div, Label } from './primitives'
import { getPreviewer } from './previewers/registry'

const UnknownFilePreview = lazy(() => import('./previewers/unknown-file-preview.tsx'))

export interface AttachmentPreviewProps {
  file: FileOut
}

/** The content of a file attachment's preview popup, dispatched by file extension */
export function AttachmentPreview({ file }: AttachmentPreviewProps) {
  const extension = getFileExtension(file.file_name)
  const Previewer = getPreviewer(extension) ?? UnknownFilePreview

  return (
    <Suspense
      fallback={
        <Div style={{ padding: 20 }}>
          <Label variant="secondary" text="Loading…" />
        </Div>
      }
    >
      <Previewer file={file} />
    </Suspense>
  )
}
