import {useState} from 'react'
import {getFileDownloadUrl} from '../../api/files/download'
import {Div, Label} from '../primitives'
import type {PreviewerProps} from './types'

/** A plain img pointed at the download URL, keeping aspect ratio as the popup resizes */
const ImagePreview = ({file}: PreviewerProps) => {
    const [failed, setFailed] = useState(false)

    if (failed) {
        return (
            <Div style={{padding: 20}}>
                <Label text="Couldn't load this image."/>
            </Div>
        )
    }

    return (
        <img
            src={getFileDownloadUrl(file.id)}
            alt={file.file_name}
            onError={() => setFailed(true)}
            style={{width: '100%', height: '100%', objectFit: 'contain', display: 'block'}}
        />
    )
};

export default ImagePreview
