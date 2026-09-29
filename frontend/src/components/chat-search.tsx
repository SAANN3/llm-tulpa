import {useEffect, useRef, useState} from 'react'

import '../styles/chat-search.scss'
import {searchMessages} from '../api/chats/search'
import type {MessageSearchOut} from '../api/chats/types'
import {Checkbox, Div, Input, Label} from './primitives'

export interface ChatSearchProps {
    chatId: number
    /** Called when a hit is clicked; the caller scrolls the chat to that message and closes the search */
    onSelect: (hit: MessageSearchOut, query: string) => void
    /** Closes the search (Escape key) */
    onClose: () => void
}

/** How long to wait after the last keystroke before running the search */
const DEBOUNCE_MS = 250
/** The most hits the results list shows */
const MAX_HITS = 50

const STORAGE_KEY_ASSISTANT = 'tulpa:search_include_assistant'
const STORAGE_KEY_USER = 'tulpa:search_include_user'
const STORAGE_KEY_THINKING = 'tulpa:search_include_thinking'
const STORAGE_KEY_TOOLS = 'tulpa:search_include_tools'

const readStorageBool = (key: string, defaultValue = true): boolean => {
    try {
        const item = localStorage.getItem(key)
        return item != null ? item === 'true' : defaultValue
    } catch {
        return defaultValue
    }
}

const writeStorageBool = (key: string, value: boolean) => {
    try {
        localStorage.setItem(key, String(value))
    } catch {
        // ignore
    }
}

/**
 * In-chat message search: debounces the input and lists the chat's messages containing it
 * (newest first), each with a snippet of the content around its match, the match itself
 * highlighted. Clicking a hit tells the caller which message to scroll to.
 */
export const ChatSearch = ({chatId, onSelect, onClose}: ChatSearchProps) => {
    const [query, setQuery] = useState('')
    const [includeAssistant, setIncludeAssistantState] = useState(() => readStorageBool(STORAGE_KEY_ASSISTANT, true))
    const [includeUser, setIncludeUserState] = useState(() => readStorageBool(STORAGE_KEY_USER, true))
    const [includeThinking, setIncludeThinkingState] = useState(() => readStorageBool(STORAGE_KEY_THINKING, true))
    const [includeTools, setIncludeToolsState] = useState(() => readStorageBool(STORAGE_KEY_TOOLS, true))
    const [matches, setMatches] = useState<MessageSearchOut[] | null>(null)
    const [total, setTotal] = useState(0)
    const [loading, setLoading] = useState(false)
    const [failed, setFailed] = useState(false)
    const requestId = useRef(0)

    const setIncludeAssistant = (val: boolean) => {
        setIncludeAssistantState(val)
        writeStorageBool(STORAGE_KEY_ASSISTANT, val)
    }

    const setIncludeUser = (val: boolean) => {
        setIncludeUserState(val)
        writeStorageBool(STORAGE_KEY_USER, val)
    }

    const setIncludeThinking = (val: boolean) => {
        setIncludeThinkingState(val)
        writeStorageBool(STORAGE_KEY_THINKING, val)
    }

    const setIncludeTools = (val: boolean) => {
        setIncludeToolsState(val)
        writeStorageBool(STORAGE_KEY_TOOLS, val)
    }

    const trimmed = query.trim()

    useEffect(() => {
        if (!trimmed) {
            requestId.current += 1
            setMatches(null)
            setTotal(0)
            setLoading(false)
            setFailed(false)
            return
        }

        const id = ++requestId.current
        setLoading(true)
        setFailed(false)

        const timer = setTimeout(async () => {
            try {
                const result = await searchMessages({
                    chatId,
                    query: trimmed,
                    limit: MAX_HITS,
                    includeAssistant,
                    includeUser,
                    includeThinking,
                    includeTools,
                })
                if (requestId.current !== id) return
                setMatches(result.matches)
                setTotal(result.total)
            } catch {
                if (requestId.current !== id) return
                setMatches([])
                setTotal(0)
                setFailed(true)
            } finally {
                if (requestId.current === id) setLoading(false)
            }
        }, DEBOUNCE_MS)

        return () => clearTimeout(timer)
    }, [chatId, trimmed, includeAssistant, includeUser, includeThinking, includeTools])

    const select = (hit: MessageSearchOut) => {
        requestId.current += 1
        onSelect(hit, trimmed)
    }

    return (
        <Div className="vbox chat-search">
            <Input
                autoFocus
                className="chat-search__input"
                placeholder="Search this chat…"
                text={query}
                onChanged={setQuery}
                onKeyDown={(e) => {
                    if (e.key === 'Escape') onClose()
                }}
            />

            <Div className="chat-search__filters">
                <label className="chat-search__filter">
                    <Checkbox toggled={includeAssistant} onToggled={setIncludeAssistant}/>
                    <Label text="Assistant"/>
                </label>
                <label className="chat-search__filter">
                    <Checkbox toggled={includeUser} onToggled={setIncludeUser}/>
                    <Label text="User"/>
                </label>
                <label className="chat-search__filter">
                    <Checkbox toggled={includeThinking} onToggled={setIncludeThinking}/>
                    <Label text="Thoughts"/>
                </label>
                <label className="chat-search__filter">
                    <Checkbox toggled={includeTools} onToggled={setIncludeTools}/>
                    <Label text="Tools"/>
                </label>
            </Div>

            {loading && <Label className="chat-search__status" text="searching…"/>}
            {!loading && failed && <Label className="chat-search__status" text="search failed" />}
            {!loading && !failed && matches && matches.length === 0 && (
                <Label className="chat-search__status" text={`no matches for “${trimmed}”`} />
            )}

            {matches && matches.length > 0 && (
                <Div className="vbox chat-search__results">
                    {matches.map((hit) => (
                        <Div key={hit.id} className="list-row chat-search__row" onClick={() => select(hit)}>
                            <Div className="chat-search__row-header">
                                <Label
                                    variant="secondary"
                                    className="chat-search__meta"
                                    text={
                                        hit.matched_in !== 'content'
                                            ? `${hit.role} · ${hit.matched_in.toUpperCase()} · ${new Date(hit.created_at).toLocaleTimeString()}`
                                            : `${hit.role} · ${new Date(hit.created_at).toLocaleTimeString()}`
                                    }
                                />
                            </Div>
                            <Div className="chat-search__snippet">
                                {hit.before ? <span className="chat-search__dim">…{hit.before}</span> : null}
                                <span className="chat-search__match">{hit.matched}</span>
                                {hit.after ? <span className="chat-search__dim">{hit.after}…</span> : null}
                            </Div>
                        </Div>
                    ))}
                    {total > matches.length && (
                        <Label className="chat-search__status" text={`…and ${total - matches.length} more`} />
                    )}
                </Div>
            )}
        </Div>
    )
};
