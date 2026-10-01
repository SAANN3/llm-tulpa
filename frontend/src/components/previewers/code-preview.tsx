import {Prism as SyntaxHighlighter} from 'react-syntax-highlighter'
import {LANGUAGE_ALIASES} from '../../utils/code-language.ts'
import {codeTheme} from '../../utils/code-theme.ts'
import {Div, Label} from '../primitives'
import type {PreviewerProps} from './types'
import {useTextContent} from './use-text-content.ts'

const getExtension = (fileName: string): string => {
    const dot = fileName.lastIndexOf('.')
    return dot === -1 ? '' : fileName.slice(dot + 1).toLowerCase()
};

const languageFor = (fileName: string): string => {
    const extension = getExtension(fileName)
    return LANGUAGE_ALIASES[extension] ?? extension
};

/** Syntax-highlighted source via Prism, in the active theme's colors */
const CodePreview = ({file}: PreviewerProps) => {
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
        <SyntaxHighlighter
            language={languageFor(file.file_name)}
            style={codeTheme}
            customStyle={{margin: 0, padding: '1em', fontSize: 13}}
            showLineNumbers
        >
            {content}
        </SyntaxHighlighter>
    )
};

export default CodePreview
