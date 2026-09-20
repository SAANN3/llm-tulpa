import {Div, Label} from '../primitives'
import type {PreviewerProps} from './types'
import {useTextContent} from './use-text-content.ts'

const BINARY_THRESHOLD = 0.01

const SNIFF_SAMPLE_CHARS = 4_000

const looksBinary = (sample: string): boolean => {
    if (sample.length === 0) return false

    let suspicious = 0
    for (const ch of sample) {
        const code = ch.codePointAt(0) ?? 0
        const isControl = code < 32 && ch !== '\n' && ch !== '\r' && ch !== '\t'
        if (isControl || code === 0xfffd) suspicious += 1
    }
    return suspicious / sample.length > BINARY_THRESHOLD
};

const unknownFileNotice = (file: PreviewerProps['file']) => (
    <Div className="vbox center" style={{gap: 6, padding: 24, minWidth: 220}}>
        <Label text="Unknown file type" style={{fontSize: 14, fontWeight: 600}}/>
        <Label variant="secondary" text={file.file_name} style={{fontSize: 12}}/>
    </Div>
);

/** Fallback previewer: shows content as plain text unless it looks like binary data */
const UnknownFilePreview = ({file}: PreviewerProps) => {
    const {content, failed} = useTextContent(file)

    if (failed) return unknownFileNotice(file)

    if (content == null) {
        return (
            <Div style={{padding: 20}}>
                <Label variant="secondary" text="Loading…"/>
            </Div>
        )
    }

    if (looksBinary(content.slice(0, SNIFF_SAMPLE_CHARS))) return unknownFileNotice(file)

    return (
        <Div style={{padding: 12}}>
      <pre style={{margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word', fontFamily: 'monospace', fontSize: 13}}>
        {content}
      </pre>
        </Div>
    )
};

export default UnknownFilePreview
