import {useMemo, useRef, useState} from 'react'
import type {ElementType, ReactNode} from 'react'
import ReactMarkdown from 'react-markdown'
import type {Components} from 'react-markdown'
import remarkBreaks from 'remark-breaks'
import remarkGfm from 'remark-gfm'
import {ChevronDown, ChevronRight} from 'pixelarticons/react'
import '../styles/chat-message.scss'
import {Attachment} from './attachment.tsx'
import {Button, Div, Label} from './primitives'
import {highlightInNode, highlightText} from '../utils/highlight.tsx'
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
}

const formatThoughtDuration = (ms: number): string => {
    const totalSeconds = ms / 1000
    if (totalSeconds < 60) return `Thought for ${totalSeconds.toFixed(1)}s`

    const minutes = Math.floor(totalSeconds / 60)
    const seconds = Math.round(totalSeconds % 60)
    return `Thought for ${minutes}m ${seconds}s`
};

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

    const markdownComponents = useMemo(() => {
        if (!highlightQuery?.trim()) return undefined
        const q = highlightQuery.trim()
        const wrap = (tag: ElementType) => {
            const Tag = tag
            return ({children, node: _node, ...props}: {children?: ReactNode; node?: unknown}) => (
                <Tag {...props}>{highlightInNode(children, q)}</Tag>
            )
        }
        return {
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
                    <Div ref={thinkingRef} className="vbox chat-message__thinking">
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
                <Label
                    variant="secondary"
                    className={`chat-message__time${isUser ? ' chat-message__time--user' : ''}`}
                    text={
                        !isUser && eval_tokens != null
                            ? `${new Date(created_at).toLocaleTimeString()}, spent ${formatTokenCount(eval_tokens)} tokens`
                            : new Date(created_at).toLocaleTimeString()
                    }
                />
            </Div>
        </Div>
    )
};
