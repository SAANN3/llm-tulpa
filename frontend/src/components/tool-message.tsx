import {ChevronDown, ChevronRight} from 'pixelarticons/react'

import '../styles/tool-message.scss'
import {Div, Label} from './primitives'

export interface ToolMessageProps {
    tool_name: string
    content: unknown
    created_at: string
    arguments?: Record<string, unknown>
    success?: boolean
    err?: string | null
    expanded: boolean
    onToggle: () => void
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

/** A bounded, scrollable, labelled block of pre-formatted JSON */
const DetailBlock = ({label, text}: { label: string; text: string }) => (
    <Div className="vbox tool-message__block">
        <Label variant="secondary" className="tool-message__block-label" text={label}/>
        <pre className="mono tool-message__pre">{text}</pre>
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
    onToggle
}: ToolMessageProps) => {
    const contentText = typeof content === 'string' ? content : JSON.stringify(content, null, 2)
    const argsText = args && Object.keys(args).length > 0 ? JSON.stringify(args, null, 2) : null
    const argsSummary = args ? describeArgs(args) : ''
    const resultChip = success === false ? 'error' : describeResult(content)

    return (
        <Div className="tool-message">
            <Div className="vbox tool-message__card">
                <Div onClick={onToggle} className="list-row tool-message__header">
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
                    <span className="tool-message__caret">
            {expanded ? <ChevronDown width={14} height={14}/> : <ChevronRight width={14} height={14}/>}
          </span>
                </Div>
                {expanded ? (
                    <Div className="vbox tool-message__detail">
                        {argsText ? <DetailBlock label="ARGUMENTS" text={argsText}/> : null}
                        <DetailBlock label="RESULT" text={contentText}/>
                        {success === false && err ?
                            <Label variant="secondary" className="tool-message__err" text={err}/> : null}
                    </Div>
                ) : null}
            </Div>
            <Label variant="secondary" className="tool-message__time" text={new Date(created_at).toLocaleTimeString()}/>
        </Div>
    )
};
