import { lazy, Suspense } from 'react'

import type { FileOut } from '../api/files/types'
import { getFileExtension } from '../utils/fileExtension'
import { Div, Label } from './primitives'
import { getPreviewer } from './previewers/registry'

const UnknownFilePreview = lazy(() => import('./previewers/UnknownFilePreview'))

export interface AttachmentPreviewProps {
  file: FileOut
}

/** The content of a file attachment's preview popup — dispatches to whichever
 * previewer (see `previewers/registry.ts`) recognizes the file's extension, or
 * `UnknownFilePreview` if none does (itself falling back further to a plain "unknown
 * file type" notice, only once it's actually checked the content isn't binary). Every
 * previewer is lazy-loaded, so e.g. the syntax-highlighting/docx-preview/xlsx
 * libraries a specific previewer needs only get downloaded once a file that actually
 * needs one is opened, never on a normal page load. Either way the file stays
 * downloadable — that's the popup shell's own header button (`WindowsPopup`'s
 * `onDownload`), not something this handles. */
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
