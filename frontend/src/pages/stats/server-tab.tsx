import type {LlamaServer, RunningModel} from '../../api/stats/types'
import {Div, Label} from '../../components/primitives'
import {StatCard} from '../../components/stats/stat-card.tsx'
import {StatsStatus} from '../../components/stats/stats-status.tsx'
import {useServerStats} from '../../hooks/use-server-stats.ts'
import {formatBytes} from '../../utils/format-bytes.ts'
import {formatTokenCount} from '../../utils/format.ts'
import {formatPercent, formatSpeed} from '../../utils/stats.ts'

/** "in 4m 12s" until an RFC 3339 instant, or null when it has passed or isn't known */
const timeUntil = (instant: string | null): string | null => {
    if (!instant) return null
    const seconds = Math.round((new Date(instant).getTime() - Date.now()) / 1000)
    if (seconds <= 0) return null
    return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${seconds % 60}s`
};

const modelHint = (model: RunningModel): string => {
    const parts = [
        model.size != null ? formatBytes(model.size) : null,
        model.size_vram != null && model.size != null && model.size > 0
            ? `${formatPercent(model.size_vram, model.size)} on GPU`
            : null,
        model.context_length != null ? `${formatTokenCount(model.context_length)} context` : null,
        timeUntil(model.expires_at) ? `unloads in ${timeUntil(model.expires_at)}` : null,
    ]
    return parts.filter(Boolean).join(' · ')
};

const LlamaServerCards = ({server}: { server: LlamaServer }) => {
    const evaluated = server.prompt_tokens_total ?? 0
    const cached = server.prompt_tokens_cached_total ?? 0
    const drafted = server.draft_tokens_total ?? 0

    return (
        <Div className="stats__cards">
            <StatCard label="Generation (average)" value={formatSpeed(server.predicted_tokens_per_second)}
                      hint={`${formatTokenCount(Math.round(server.predicted_tokens_total ?? 0))} tokens since start`}/>
            <StatCard label="Prompt (average)" value={formatSpeed(server.prompt_tokens_per_second)}
                      hint={`${formatTokenCount(Math.round(evaluated))} evaluated, ${formatTokenCount(Math.round(cached))} from cache`}/>
            {drafted > 0 ? (
                <StatCard label="Drafts accepted"
                          value={formatPercent(server.draft_tokens_accepted_total ?? 0, drafted)}
                          hint={`${formatTokenCount(Math.round(drafted))} drafted`}/>
            ) : null}
            <StatCard label="Working on" value={`${server.slots_processing ?? 0} of ${server.slots_total ?? 1}`}/>
        </Div>
    )
};

/** What the model backend is running right now, and how full the biggest chats are */
export const ServerTab = () => {
    const {server, context} = useServerStats()
    if (!server) return <StatsStatus/>

    return (
        <>
            <Label text="Model backend"/>
            {server.state === 'stopped' ? (
                <Label variant="secondary" text="The model server is stopped; it starts when a chat needs it."/>
            ) : server.state === 'starting' ? (
                <Label variant="secondary" text="The model server is loading a model."/>
            ) : server.state === 'failed' || server.state === 'not_installed' ? (
                <Label variant="secondary" text={server.detail ?? 'The model server is not available.'}/>
            ) : !server.reachable ? (
                <Label variant="secondary" text="The model backend isn't answering."/>
            ) : server.models.length === 0 ? (
                <Label variant="secondary" text="Online, with no model loaded right now."/>
            ) : (
                <Div className="stats__cards">
                    {server.models.map((model) => (
                        <StatCard key={model.name} label="Loaded" value={model.name} hint={modelHint(model)}/>
                    ))}
                </Div>
            )}
            {server.llama_server ? <LlamaServerCards server={server.llama_server}/> : null}

            <Label text="Chat context"/>
            {context == null ? (
                <StatsStatus/>
            ) : (
                <>
                    <Label variant="secondary" className="stats__note"
                           text={`${context.compacted_chats} of ${context.total_chats} chats have been compacted. The window is ${formatTokenCount(context.context_length)} tokens.`}/>
                    <Div className="context-list">
                        {context.largest.map((chat) => (
                            <Div key={chat.chat_id} className="context-list__row">
                                <Label className="context-list__name" text={chat.name}/>
                                <Div className="context-list__bar">
                                    <Div className="context-list__fill"
                                         style={{width: `${Math.min(100, (chat.prompt_tokens / context.context_length) * 100)}%`}}/>
                                </Div>
                                <Label variant="secondary" className="context-list__value"
                                       text={`${formatTokenCount(chat.prompt_tokens)} · ${formatPercent(chat.prompt_tokens, context.context_length)}`}/>
                            </Div>
                        ))}
                    </Div>
                </>
            )}
        </>
    )
};
