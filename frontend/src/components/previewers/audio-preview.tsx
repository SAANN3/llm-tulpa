import {useFileBlobUrl} from '../../hooks/use-file-blob-url.ts'
import {Div, Label} from '../primitives'
import type {PreviewerProps} from './types'

/** An audio control fed from the file's blob */
const AudioPreview = ({file}: PreviewerProps) => {
    const {url, failed} = useFileBlobUrl(file.id)

    return (
        <Div style={{padding: 24}}>
            {url ? (
                <audio src={url} controls style={{width: '100%'}}/>
            ) : (
                <Label variant={failed ? undefined : 'secondary'} text={failed ? "Couldn't load this audio." : 'Loading…'}/>
            )}
        </Div>
    )
};

export default AudioPreview
