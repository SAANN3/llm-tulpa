import {useFileBlobUrl} from '../../hooks/use-file-blob-url.ts'
import {Div, Label} from '../primitives'
import type {PreviewerProps} from './types'

/** An img fed from the file's blob, keeping aspect ratio as the popup resizes */
const ImagePreview = ({file}: PreviewerProps) => {
    const {url, failed} = useFileBlobUrl(file.id)

    if (failed) {
        return (
            <Div style={{padding: 20}}>
                <Label text="Couldn't load this image."/>
            </Div>
        )
    }

    if (!url) {
        return (
            <Div style={{padding: 20}}>
                <Label variant="secondary" text="Loading…"/>
            </Div>
        )
    }

    return (
        <img
            src={url}
            alt={file.file_name}
            style={{width: '100%', height: '100%', objectFit: 'contain', display: 'block'}}
        />
    )
};

export default ImagePreview
