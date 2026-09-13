import type { ComponentType } from 'react'

import type { FileOut } from '../../api/files/types'

export interface PreviewerProps {
  file: FileOut
}

/** One extension's preview implementation — `AttachmentPreview` renders whichever one
 * `previewers/registry.ts` maps a file's extension to. Always a default export in its
 * own file (see `registry.ts`'s `React.lazy` calls) so the (sometimes large — syntax
 * highlighting, docx-preview, xlsx) library it needs only gets downloaded once a file
 * that actually needs it is opened. */
export type Previewer = ComponentType<PreviewerProps>
