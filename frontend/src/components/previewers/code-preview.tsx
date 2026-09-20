import {Prism as SyntaxHighlighter} from 'react-syntax-highlighter'
import {oneDark, oneLight} from 'react-syntax-highlighter/dist/esm/styles/prism'
import {useTheme} from '../../context/use-theme.ts'
import {Div, Label} from '../primitives'
import type {PreviewerProps} from './types'
import {useTextContent} from './use-text-content.ts'

const LANGUAGE_ALIASES: Record<string, string> = {
    rs: 'rust',
    ts: 'typescript',
    js: 'javascript',
    mjs: 'javascript',
    cjs: 'javascript',
    py: 'python',
    sh: 'bash',
    zsh: 'bash',
    yml: 'yaml',
    kt: 'kotlin',
    kts: 'kotlin',
    rb: 'ruby',
    md: 'markdown',
    h: 'c',
    hpp: 'cpp',
    cc: 'cpp',
    hs: 'haskell',
    cs: 'csharp',
}

const getExtension = (fileName: string): string => {
    const dot = fileName.lastIndexOf('.')
    return dot === -1 ? '' : fileName.slice(dot + 1).toLowerCase()
};

const languageFor = (fileName: string): string => {
    const extension = getExtension(fileName)
    return LANGUAGE_ALIASES[extension] ?? extension
};

/** Syntax-highlighted source via Prism, following the app's light/dark theme */
const CodePreview = ({file}: PreviewerProps) => {
    const {content, failed} = useTextContent(file)
    const {themeName} = useTheme()

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
            style={themeName === 'white' ? oneLight : oneDark}
            customStyle={{margin: 0, fontSize: 13}}
            showLineNumbers
        >
            {content}
        </SyntaxHighlighter>
    )
};

export default CodePreview
