import {getFileDownloadUrl} from '../../api/files/download'
import type {PreviewerProps} from './types'

/** A plain video control pointed at the download URL */
const VideoPreview = ({file}: PreviewerProps) => (
    <video
        src={getFileDownloadUrl(file.id)}
        controls
        style={{width: '100%', height: '100%', display: 'block', background: 'black'}}
    />
);

export default VideoPreview
