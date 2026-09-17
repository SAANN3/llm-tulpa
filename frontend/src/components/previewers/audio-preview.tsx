import {getFileDownloadUrl} from '../../api/files/download'
import {Div} from '../primitives'
import type {PreviewerProps} from './types'

/** A plain audio control pointed at the download URL */
const AudioPreview = ({file}: PreviewerProps) => (
    <Div style={{padding: 24}}>
        <audio src={getFileDownloadUrl(file.id)} controls style={{width: '100%'}}/>
    </Div>
);

export default AudioPreview
