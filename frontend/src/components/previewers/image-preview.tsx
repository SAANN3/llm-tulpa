import {useFileBlobUrl} from '../../hooks/use-file-blob-url.ts'
import {Div, Label} from '../primitives'
import type {PreviewerProps} from './types'
import {ZoomableImage} from './zoomable-image.tsx'

/** An image fed from the file's blob, fitted to the window or zoomed */
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

    return <ZoomableImage src={url} alt={file.file_name}/>
};

export default ImagePreview
