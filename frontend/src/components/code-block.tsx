import {Children, isValidElement} from 'react'
import type {ReactNode} from 'react'
import {Prism as SyntaxHighlighter} from 'react-syntax-highlighter'
import {LANGUAGE_ALIASES} from '../utils/code-language.ts'
import {codeTheme} from '../utils/code-theme.ts'

/** react-markdown's `pre`: a fenced block that names a language is rendered through Prism in the
 * active theme's colors (`codeTheme`), on the same background as any other code block; anything
 * else (no language, or an indented block) stays the plain `<pre>`. Taken at `pre` rather than
 * `code` so the highlighter's own `<pre>` isn't nested inside the one react-markdown already emits. */
export const CodeBlock = ({children}: { children?: ReactNode }) => {
    const code = Children.toArray(children)[0]
    const tag = isValidElement<{ className?: string; children?: ReactNode }>(code)
        ? /language-([\w-]+)/.exec(code.props.className ?? '')?.[1]
        : undefined

    if (!isValidElement<{ children?: ReactNode }>(code) || !tag) return <pre>{children}</pre>

    return (
        <SyntaxHighlighter
            language={LANGUAGE_ALIASES[tag] ?? tag}
            style={codeTheme}
        >
            {String(code.props.children).replace(/\n$/, '')}
        </SyntaxHighlighter>
    )
};
