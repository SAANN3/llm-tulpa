import {useFileBlobUrl} from '../../hooks/use-file-blob-url.ts'
import {Div, Label} from '../primitives'
import type {PreviewerProps} from './types'

/** A video control fed from the file's blob (the whole file is fetched before it can play) */
const VideoPreview = ({file}: PreviewerProps) => {
    const {url, failed} = useFileBlobUrl(file.id)

    if (failed || !url) {
        return (
            <Div style={{padding: 20}}>
                <Label variant={failed ? undefined : 'secondary'} text={failed ? "Couldn't load this video." : 'Loading…'}/>
            </Div>
        )
    }

    return <video src={url} controls style={{width: '100%', height: '100%', display: 'block', background: 'black'}}/>
};

export default VideoPreview
