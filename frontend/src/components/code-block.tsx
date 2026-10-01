import {Children, isValidElement, useEffect, useRef, useState} from 'react'
import type {ReactNode} from 'react'
import {Check, Copy} from 'pixelarticons/react'
import {Prism as SyntaxHighlighter} from 'react-syntax-highlighter'
import '../styles/code-block.scss'
import {copyText} from '../utils/copy-text.ts'
import {LANGUAGE_ALIASES} from '../utils/code-language.ts'
import {codeTheme} from '../utils/code-theme.ts'
import {Button, Div} from './primitives'

/** The text inside a node: react-markdown hands a code element a plain string, but a wrapper
 * component (the search highlighter) can hand over nested nodes */
const textOf = (node: ReactNode): string => {
    if (typeof node === 'string' || typeof node === 'number') return String(node)
    if (Array.isArray(node)) return node.map(textOf).join('')
    return isValidElement<{ children?: ReactNode }>(node) ? textOf(node.props.children) : ''
};

export interface CodeBlockProps {
    children?: ReactNode
    /** Skip syntax highlighting and keep the plain `<pre>` — while a chat search is active, so the
     * matches inside the code stay visible (the highlighter's token spans would hide them) */
    plain?: boolean
}

/** react-markdown's `pre`, with a button that copies the block's code. A fenced block that names a
 * language is rendered through Prism in the active theme's colors (`codeTheme`), on the same
 * background as any other code block; anything else (no language, or an indented block) stays the
 * plain `<pre>`. Taken at `pre` rather than `code` so the highlighter's own `<pre>` isn't nested
 * inside the one react-markdown already emits. */
export const CodeBlock = ({children, plain = false}: CodeBlockProps) => {
    const [copied, setCopied] = useState(false)
    const copiedTimer = useRef<number>(undefined)
    useEffect(() => () => window.clearTimeout(copiedTimer.current), [])

    const code = Children.toArray(children)[0]
    const codeElement = isValidElement<{ className?: string; children?: ReactNode }>(code) ? code : null
    const text = textOf(codeElement ? codeElement.props.children : children).replace(/\n$/, '')
    const language = codeElement ? /language-([\w-]+)/.exec(codeElement.props.className ?? '')?.[1] : undefined

    const handleCopy = async () => {
        if (!(await copyText(text))) return
        setCopied(true)
        window.clearTimeout(copiedTimer.current)
        copiedTimer.current = window.setTimeout(() => setCopied(false), 2000)
    }

    return (
        <Div className="code-block">
            {plain || !language ? (
                <pre>{children}</pre>
            ) : (
                <SyntaxHighlighter language={LANGUAGE_ALIASES[language] ?? language} style={codeTheme}>
                    {text}
                </SyntaxHighlighter>
            )}
            <Button className="code-block__copy" variant="secondary" title="Copy code" onClicked={handleCopy}>
                {copied ? <Check width={13} height={13}/> : <Copy width={13} height={13}/>}
            </Button>
        </Div>
    )
};
