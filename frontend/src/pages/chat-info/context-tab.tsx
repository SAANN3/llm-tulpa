import type {ChatContextResponse, ChatOut, ContextPartKind, ContextPartOut} from '../../api/chats/types.ts'
import {Div, Label} from '../../components/primitives'
import {useFormatTime} from '../../hooks/use-format-time.ts'
import {formatTokenCount} from '../../utils/format.ts'

const NAMES: Record<ContextPartKind, string> = {
    system_prompt: 'System prompt',
    tools: 'Tool definitions',
    key_facts: 'Goal & key facts',
    pinned: 'Your early messages',
    summary: 'Summary',
    notes: 'Notes',
    user_messages: 'Your messages',
    replies: 'Replies',
    thinking: 'Thinking',
    tool_calls: 'Tool calls',
    tool_results: 'Tool results',
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`

/** The dim note after a part's name: what it is, and how many */
const noteFor = (part: ContextPartOut, context: ChatContextResponse): string => {
    const n = part.count ?? 0
    const since = context.memory.folded ? ' since the fold' : ''
    switch (part.kind) {
        case 'system_prompt':
            return context.memory.folded ? 'the rules, and the note on what the summary replaced' : 'the rules the model works by'
        case 'tools':
            return `${plural(n, 'tool')}, sent with every call`
        case 'key_facts':
            return `${plural(n, 'entry', 'entries')}, kept across folds`
        case 'pinned':
            return 'what you wrote before the fold, word for word'
        case 'summary':
            return `of the ${plural(context.folded_messages, 'message')} before the fold`
        case 'notes':
            return "the model's own working notes"
        case 'user_messages':
            return `${n}${since}, the backend's notices included`
        case 'replies':
            return plural(n, 'reply', 'replies')
        case 'thinking':
            return `${plural(n, 'trace')} replayed`
        case 'tool_calls':
            return plural(n, 'call')
        case 'tool_results':
            return context.cleared_results > 0
                ? `${plural(n, 'result')}; ${context.cleared_results} older cleared to a line, saving about ${formatTokenCount(context.cleared_tokens)}`
                : plural(n, 'result')
    }
}

/** The parts get the accent in steps, strongest first, so neighbours in the bar are told apart without a palette */
const colorAt = (i: number, of: number) =>
    `color-mix(in srgb, var(--color-tertiary) ${Math.round(92 - (i / Math.max(1, of - 1)) * 76)}%, var(--color-secondary))`

/** How full the context is and with what, as the next turn would send it; and the chat's own numbers */
export const ContextTab = ({chat, context}: { chat: ChatOut, context: ChatContextResponse }) => {
    const formatTime = useFormatTime()
    const parts = context.parts.filter((part) => part.tokens > 0)
    const max = context.context_length
    const percent = (n: number, of: number) => `${of > 0 ? Math.round(n / of * 100) : 0}%`
    const when = (at: string) => `${new Date(at).toLocaleDateString()}, ${formatTime(at)}`
    const since = context.messages - context.folded_messages

    return (
        <Div className="chat-info__stack">
            <Div className="chat-info__pane">
                <Div className="chat-info__pane-title">
                    <span>Context</span>
                    <span className="chat-info__spacer"/>
                    <span className="chat-info__dim">
                        {context.measured ? "from the model's last call" : 'estimated: nothing measured since the last change'}
                    </span>
                </Div>
                <Div>
                    <span className="chat-info__big">{formatTokenCount(context.used)}</span>
                    <span className="chat-info__dim">
                        {` of ${formatTokenCount(max)} · ${percent(context.used, max)} · folds at ${percent(context.fold_at, max)}`}
                    </span>
                </Div>
                <Div className="chat-info__gauge">
                    {parts.map((part, i) => (
                        <i key={part.kind} title={NAMES[part.kind]}
                           style={{width: `${part.tokens / max * 100}%`, background: colorAt(i, parts.length)}}/>
                    ))}
                    <b className="chat-info__fold-mark" style={{left: `${context.fold_at / max * 100}%`}} title="The chat folds here"/>
                </Div>
                <Div className="chat-info__legend">
                    {parts.map((part, i) => (
                        <Div key={part.kind} className="chat-info__legend-row">
                            <span className="chat-info__swatch" style={{background: colorAt(i, parts.length)}}/>
                            <span>{NAMES[part.kind]}</span>
                            <span className="chat-info__note">{noteFor(part, context)}</span>
                            <span className="chat-info__num">{formatTokenCount(part.tokens)}</span>
                            <span className="chat-info__share">{percent(part.tokens, context.used)}</span>
                        </Div>
                    ))}
                </Div>
            </Div>
            <Div className="chat-info__pane">
                <Div className="chat-info__pane-title"><span>Details</span></Div>
                <Div className="chat-info__details">
                    <Label variant="secondary" text="Model"/><span>{chat.model}</span>
                    <Label variant="secondary" text="Tools"/><span>{chat.tools_enabled ? 'on' : 'off'}</span>
                    <Label variant="secondary" text="Started"/><span>{when(chat.created_at)}</span>
                    <Label variant="secondary" text="Last active"/><span>{when(chat.updated_at)}</span>
                    <Label variant="secondary" text="Messages"/>
                    <span>{context.memory.folded ? `${context.messages} · ${since} since the last fold` : context.messages}</span>
                    <Label variant="secondary" text="Generated"/><span>{`${formatTokenCount(context.generated_tokens)} tokens`}</span>
                    <Label variant="secondary" text="Sub-agents"/><span>{plural(context.subagent_chats, 'chat')}</span>
                </Div>
            </Div>
        </Div>
    )
};
