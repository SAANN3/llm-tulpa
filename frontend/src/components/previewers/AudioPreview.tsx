import { getFileDownloadUrl } from '../../api/files/download'
import { Div } from '../primitives'
import type { PreviewerProps } from './types'

/** A plain `<audio controls>` pointed straight at the download URL — same reasoning as
 * `ImagePreview`/`VideoPreview`. No explicit size: an audio control bar has a natural
 * height, unlike a visual media previewer that wants to fill the popup. */
export default function AudioPreview({ file }: PreviewerProps) {
  return (
    <Div style={{ padding: 24 }}>
      <audio src={getFileDownloadUrl(file.id)} controls style={{ width: '100%' }} />
    </Div>
  )
}
