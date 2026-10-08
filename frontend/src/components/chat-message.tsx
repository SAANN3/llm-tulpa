import {useEffect, useMemo, useRef, useState} from 'react'
import type {ElementType, ReactNode} from 'react'
import ReactMarkdown from 'react-markdown'
import type {Components} from 'react-markdown'
import remarkBreaks from 'remark-breaks'
import remarkGfm from 'remark-gfm'
import {Check, ChevronDown, ChevronRight, Copy, Pencil, Reload, Trash} from 'pixelarticons/react'
import '../styles/chat-message.scss'
import {Attachment} from './attachment.tsx'
import {CodeBlock} from './code-block.tsx'
import {Button, Div, Label} from './primitives'
import {highlightInNode, highlightText} from '../utils/highlight.tsx'
import {copyText} from '../utils/copy-text.ts'
import {formatTokenCount} from '../utils/format.ts'

export interface ChatMessageProps {
    role: 'user' | 'assistant'
    content: string
    created_at: string
    thinking?: string | null
    thought_duration_ms?: number | null
    images?: string[]
    file_ids?: number[]
    highlightQuery?: string | null
    thinkingExpanded?: boolean
    onToggleThinking?: () => void
    /**
     * Wraps the actual toggle so collapsing/expanding a thinking trace keeps the toggle
     * button itself pinned on screen, instead of the surrounding scroll container's default
     * "hold distance from the bottom" behavior (correct for content growing off-screen, wrong
     * for a resize you're scrolled into the middle of — see `LazyListHandle.preserveViewportPosition`).
     * Falls back to calling the toggle directly when absent (e.g. outside a `LazyList`).
     */
    preserveScrollFor?: (anchorEl: HTMLElement, mutate: () => void) => void
    /** How many tokens generating this reply cost (Ollama `eval_count`) — shown under the bubble */
    eval_tokens?: number | null
    /** Set only on the one reply that can be answered again; shows a button that does so */
    onRegenerate?: () => void
    /** Set only on a user message that can be edited: its text goes back into the composer */
    onEdit?: () => void
    /** Set only on a message that can be deleted, together with everything after it */
    onDelete?: () => void
}

const formatThoughtDuration = (ms: number): string => {
    const totalSeconds = ms / 1000
    if (totalSeconds < 60) return `Thought for ${totalSeconds.toFixed(1)}s`

    const minutes = Math.floor(totalSeconds / 60)
    const seconds = Math.round(totalSeconds % 60)
    return `Thought for ${minutes}m ${seconds}s`
};

// Search highlighting wraps text nodes, which Prism's token spans would hide, so while a search is
// active code blocks stay plain (keeping their copy button) and their matches stay visible.
const PlainCodeBlock = ({children}: { children?: ReactNode }) => <CodeBlock plain>{children}</CodeBlock>;

/** One chat message bubble, aligned by role */
export const ChatMessage = ({
    role,
    content,
    created_at,
    thinking,
    thought_duration_ms,
    images,
    file_ids,
    highlightQuery,
    thinkingExpanded,
    onToggleThinking,
    preserveScrollFor,
    eval_tokens,
    onRegenerate,
    onEdit,
    onDelete,
}: ChatMessageProps) => {
    const isUser = role === 'user'
    const [localThinking, setLocalThinking] = useState(false)
    const isThinkingOpen = thinkingExpanded !== undefined ? thinkingExpanded : localThinking
    const thinkingRef = useRef<HTMLDivElement>(null)
    const toggleThinking = onToggleThinking ?? (() => setLocalThinking((v) => !v))
    const handleToggleThinking = () => {
        const anchor = thinkingRef.current
        if (anchor && preserveScrollFor) preserveScrollFor(anchor, toggleThinking)
        else toggleThinking()
    }

    const [copied, setCopied] = useState(false)
    const copiedTimer = useRef<number>(undefined)
    useEffect(() => () => window.clearTimeout(copiedTimer.current), [])
    const handleCopy = async () => {
        if (!(await copyText(content))) return
        setCopied(true)
        window.clearTimeout(copiedTimer.current)
        copiedTimer.current = window.setTimeout(() => setCopied(false), 2000)
    }

    const markdownComponents = useMemo(() => {
        if (!highlightQuery?.trim()) return {pre: CodeBlock} satisfies Components
        const q = highlightQuery.trim()
        const wrap = (tag: ElementType) => {
            const Tag = tag
            return ({children, node: _node, ...props}: {children?: ReactNode; node?: unknown}) => (
                <Tag {...props}>{highlightInNode(children, q)}</Tag>
            )
        }
        return {
            pre: PlainCodeBlock,
            p: wrap('p'),
            li: wrap('li'),
            h1: wrap('h1'),
            h2: wrap('h2'),
            h3: wrap('h3'),
            h4: wrap('h4'),
            h5: wrap('h5'),
            h6: wrap('h6'),
            blockquote: wrap('blockquote'),
            code: wrap('code'),
            td: wrap('td'),
            th: wrap('th'),
            span: wrap('span'),
            strong: wrap('strong'),
            em: wrap('em'),
            a: wrap('a'),
        } satisfies Components
    }, [highlightQuery])

    return (
        <Div className={`chat-message ${isUser ? 'chat-message--user' : 'chat-message--assistant'}`}>
            <Div className={isUser ? 'chat-message__bubble' : 'vbox chat-message__body'}>
                {(images && images.length > 0) || (file_ids && file_ids.length > 0) ? (
                    <Div className={`chat-message__attachments${content ? ' chat-message__attachments--spaced' : ''}`}>
                        {images?.map((image, index) => <Attachment key={`image-${index}`} kind="image" image={image}
                                                                   size={140}/>)}
                        {file_ids?.map((id) => <Attachment key={`file-${id}`} kind="file" fileId={id} size={140}/>)}
                    </Div>
                ) : null}
                <div className="markdown">
                    <ReactMarkdown
                        remarkPlugins={[remarkGfm, remarkBreaks]}
                        components={markdownComponents}
                    >
                        {content}
                    </ReactMarkdown>
                </div>
                {thinking ? (
                    <Div ref={thinkingRef} className={`vbox chat-message__thinking${isThinkingOpen ? ' chat-message__thinking--open' : ''}`}>
                        <Button
                            className="chat-message__thinking-toggle"
                            variant="secondary"
                            onClicked={handleToggleThinking}
                        >
                            {isThinkingOpen ? <ChevronDown width={13} height={13}/> :
                                <ChevronRight width={13} height={13}/>}
                            <span>{thought_duration_ms != null ? formatThoughtDuration(thought_duration_ms) : 'Thinking'}</span>
                        </Button>
                        {isThinkingOpen ? (
                            <Div className="chat-message__thinking-body">
                                {highlightText(thinking, highlightQuery)}
                            </Div>
                        ) : null}
                    </Div>
                ) : thought_duration_ms != null ? (
                    <Label variant="secondary" className="chat-message__thought"
                           text={formatThoughtDuration(thought_duration_ms)}/>
                ) : null}
                <Div className={`chat-message__footer${isUser ? ' chat-message__footer--user' : ''}`}>
                    <Label
                        variant="secondary"
                        className="chat-message__time"
                        text={
                            !isUser && eval_tokens != null
                                ? `${new Date(created_at).toLocaleTimeString()}, spent ${formatTokenCount(eval_tokens)} tokens`
                                : new Date(created_at).toLocaleTimeString()
                        }
                    />
                    <Button className="chat-message__copy" variant="secondary" title="Copy message" onClicked={handleCopy}>
                        {copied ? <Check width={16} height={16}/> : <Copy width={16} height={16}/>}
                    </Button>
                    {onEdit ? (
                        <Button className="chat-message__copy" variant="secondary" title="Edit message" onClicked={onEdit}>
                            <Pencil width={16} height={16}/>
                        </Button>
                    ) : null}
                    {onRegenerate ? (
                        <Button className="chat-message__copy" variant="secondary" title="Regenerate reply" onClicked={onRegenerate}>
                            <Reload width={16} height={16}/>
                        </Button>
                    ) : null}
                    {onDelete ? (
                        <Button className="chat-message__copy" variant="secondary" title="Delete from here" onClicked={onDelete}>
                            <Trash width={16} height={16}/>
                        </Button>
                    ) : null}
                </Div>
            </Div>
        </Div>
    )
};
