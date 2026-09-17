import {Div, Label} from '../primitives'
import type {PreviewerProps} from './types'
import {useTextContent} from './use-text-content.ts'

/** Plain-text content with no syntax highlighting */
const TextPreview = ({file}: PreviewerProps) => {
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
        <Div style={{padding: 12}}>
      <pre style={{margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word', fontFamily: 'monospace', fontSize: 13}}>
        {content}
      </pre>
        </Div>
    )
};

export default TextPreview
