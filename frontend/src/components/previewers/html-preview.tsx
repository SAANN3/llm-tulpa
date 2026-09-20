import {Div, Label} from '../primitives'
import type {PreviewerProps} from './types'
import {useTextContent} from './use-text-content.ts'

/** Renders HTML for real but fully sandboxed, since this is untrusted uploaded content */
const HtmlPreview = ({file}: PreviewerProps) => {
    const {content, failed} = useTextContent(file)

    if (failed) {
        return (
            <Div style={{padding: 20}}>
                <Label text="Couldn't load this file's content."/>
            </Div>
        )
    }

    if (content == null) {
        return (
            <Div style={{padding: 20}}>
                <Label variant="secondary" text="Loading…"/>
            </Div>
        )
    }

    return (
        <iframe
            srcDoc={content}
            sandbox=""
            title={file.file_name}
            style={{width: '100%', height: '100%', border: 'none', display: 'block', background: 'white'}}
        />
    )
};

export default HtmlPreview
