import {useRef} from 'react'
import {ChevronDown, ChevronRight, ExternalLink} from 'pixelarticons/react'
import {useNavigate} from 'react-router-dom'

import '../styles/tool-message.scss'
import {Button, Div, Label} from './primitives'
import {highlightText} from '../utils/highlight.tsx'

export interface ToolMessageProps {
    tool_name: string
    content: unknown
    created_at: string
    arguments?: Record<string, unknown>
    success?: boolean
    err?: string | null
    expanded: boolean
    onToggle: () => void
    /** Same as `ChatMessage`'s prop of the same name — keeps the header pinned on screen
     * when collapsing pulls its expanded detail out from under the current scroll position. */
    preserveScrollFor?: (anchorEl: HTMLElement, mutate: () => void) => void
    highlightQuery?: string | null
}

const ID_KEYS = ['path', 'file', 'filename', 'dir', 'directory', 'url', 'query', 'command', 'name', 'id']
const trunc = (s: string, n: number) => (s.length > n ? s.slice(0, n - 1) + '…' : s)

/** One-line stand-in for a tool call's arguments: the most identifying scalar in them */
const describeArgs = (args: Record<string, unknown>): string => {
    for (const k of ID_KEYS) {
        const v = args[k]
        if (typeof v === 'string' && v) return trunc(v, 60)
    }
    for (const [k, v] of Object.entries(args)) {
        if (typeof v === 'string' && v.length <= 80) return trunc(v, 60)
        if (typeof v === 'number' || typeof v === 'boolean') return `${k} ${v}`
    }
    const n = Object.keys(args).length
    return n ? `${n} field${n === 1 ? '' : 's'}` : ''
};

/** Short status chip for a tool result: an error, a true flag, a count, or nothing */
const describeResult = (content: unknown): string => {
    if (content === null || typeof content !== 'object') return trunc(String(content), 24)
    const obj = content as Record<string, unknown>
    if (obj.error) return 'error'
    for (const [k, v] of Object.entries(obj)) if (v === true) return k
    for (const [k, v] of Object.entries(obj)) if (typeof v === 'number') return `${k} ${v}`
    return 'ok'
};

/** The chat a finished `llm.run_agent` call started, when this result is one. The result reaches
 * here as an object when it arrives live and as its JSON text when it is loaded from history. */
const subChatIdOf = (toolName: string, content: unknown): number | null => {
    if (toolName !== 'llm.run_agent') return null
    let value = content
    if (typeof value === 'string') {
        try {
            value = JSON.parse(value)
        } catch {
            return null
        }
    }
    const id = value !== null && typeof value === 'object' ? (value as Record<string, unknown>).sub_chat_id : null
    return typeof id === 'number' ? id : null
};

/** A bounded, scrollable, labelled block of pre-formatted JSON */
const DetailBlock = ({label, text, highlightQuery}: { label: string; text: string; highlightQuery?: string | null }) => (
    <Div className="vbox tool-message__block">
        <Label variant="secondary" className="tool-message__block-label" text={label}/>
        <pre className="mono tool-message__pre">{highlightText(text, highlightQuery)}</pre>
    </Div>
);

/** Reports one tool call's result, collapsed to a summary by default */
export const ToolMessage = ({
    tool_name,
    content,
    created_at,
    arguments: args,
    success,

    err,
    expanded,
    onToggle,
    preserveScrollFor,
    highlightQuery
}: ToolMessageProps) => {
    const contentText = typeof content === 'string' ? content : JSON.stringify(content, null, 2)
    const argsText = args && Object.keys(args).length > 0 ? JSON.stringify(args, null, 2) : null
    const argsSummary = args ? describeArgs(args) : ''
    const resultChip = success === false ? 'error' : describeResult(content)
    const navigate = useNavigate()
    const subChatId = subChatIdOf(tool_name, content)
    const cardRef = useRef<HTMLDivElement>(null)
    const handleToggle = () => {
        const anchor = cardRef.current
        if (anchor && preserveScrollFor) preserveScrollFor(anchor, onToggle)
        else onToggle()
    }

    return (
        <Div className="tool-message">
            <Div ref={cardRef} className="vbox tool-message__card">
                <Div onClick={handleToggle} className="list-row tool-message__header">
                    <span className="tool-message__dot"/>
                    <Label className="mono tool-message__name" text={tool_name}/>
                    {!expanded ? (
                        <Label variant="secondary" className="mono tool-message__summary" text={argsSummary}/>
                    ) : (
                        <Div className="tool-message__spacer"/>
                    )}
                    {!expanded && resultChip ? (
                        <Label variant="secondary" className="mono tool-message__chip" text={resultChip}/>
                    ) : null}
                    {subChatId != null ? (
                        <Button variant="secondary" className="tool-message__open" title="Open the sub-agent's chat"
                                onClicked={(event) => {
                                    // The header toggles the card; this button only navigates.
                                    event.stopPropagation()
                                    navigate(`/chat?id=${subChatId}`)
                                }}>
                            <ExternalLink width={14} height={14}/>
                            <span>sub-agent chat</span>
                        </Button>
                    ) : null}
                    <span className="tool-message__caret">
            {expanded ? <ChevronDown width={14} height={14}/> : <ChevronRight width={14} height={14}/>}
          </span>
                </Div>
                {expanded ? (
                    <Div className="vbox tool-message__detail">
                        {argsText ? <DetailBlock label="ARGUMENTS" text={argsText} highlightQuery={highlightQuery}/> : null}
                        <DetailBlock label="RESULT" text={contentText} highlightQuery={highlightQuery}/>
                        {success === false && err ?
                            <Label variant="secondary" className="tool-message__err" text={err}/> : null}
                    </Div>
                ) : null}
            </Div>
            <Label variant="secondary" className="tool-message__time" text={new Date(created_at).toLocaleTimeString()}/>
        </Div>
    )
};
