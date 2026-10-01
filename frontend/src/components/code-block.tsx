import {Children, isValidElement} from 'react'
import type {ReactNode} from 'react'
import {Prism as SyntaxHighlighter} from 'react-syntax-highlighter'
import {oneDark, oneLight} from 'react-syntax-highlighter/dist/esm/styles/prism'
import {useTheme} from '../context/use-theme.ts'
import {LANGUAGE_ALIASES} from '../utils/code-language.ts'

/** react-markdown's `pre`: a fenced block that names a language is rendered through Prism,
 * following the app's light/dark theme like the file previews do; anything else (no language,
 * or an indented block) stays the plain `<pre>`. Taken at `pre` rather than `code` so the
 * highlighter's own `<pre>` isn't nested inside the one react-markdown already emits. */
export const CodeBlock = ({children}: { children?: ReactNode }) => {
    const {themeName} = useTheme()
    const code = Children.toArray(children)[0]
    const tag = isValidElement<{ className?: string; children?: ReactNode }>(code)
        ? /language-([\w-]+)/.exec(code.props.className ?? '')?.[1]
        : undefined

    if (!isValidElement<{ children?: ReactNode }>(code) || !tag) return <pre>{children}</pre>

    return (
        <SyntaxHighlighter
            language={LANGUAGE_ALIASES[tag] ?? tag}
            style={themeName === 'white' ? oneLight : oneDark}
            customStyle={{margin: 0, fontSize: 13}}
        >
            {String(code.props.children).replace(/\n$/, '')}
        </SyntaxHighlighter>
    )
};
