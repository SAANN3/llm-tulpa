import { getFileDownloadUrl } from '../../api/files/download'
import type { PreviewerProps } from './types'

/** A plain `<video controls>` pointed straight at the download URL — same reasoning as
 * `ImagePreview`: an embedded media element loads a resource to render, so
 * `Content-Disposition: attachment` on that route doesn't force a download here. */
export default function VideoPreview({ file }: PreviewerProps) {
  return (
    <video
      src={getFileDownloadUrl(file.id)}
      controls
      style={{ width: '100%', height: '100%', display: 'block', background: 'black' }}
    />
  )
}
