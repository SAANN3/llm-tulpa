import {useEffect, useRef, useState} from 'react'

import '../styles/chat-search.scss'
import '../styles/chat-finder.scss'
import '../styles/tree.scss'
import {findChats} from '../api/chats/find'
import type {ChatFindOut, MessageSearchOut} from '../api/chats/types'
import {highlightText} from '../utils/highlight.tsx'
import {Checkbox, Div, Input, Label} from './primitives'
import {useFormatTime} from '../hooks/use-format-time.ts'

/** What a click picked: a chat, or one of its messages (with the query that found it) */
export interface FoundTarget {
    chatId: number
    jump?: { hit: MessageSearchOut; query: string }
}

export interface ChatFinderProps {
    query: string
    onQueryChanged: (query: string) => void
    onSelect: (target: FoundTarget) => void
}

/** How long to wait after the last keystroke before running the search */
const DEBOUNCE_MS = 250

const STORAGE_KEY_MESSAGES = 'tulpa:find_chats_messages'

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
 * Searches every chat of the user at once: by name, and (the "Messages" toggle, on by default)
 * by what was said in them. Results are grouped by chat, most recently active first, each with
 * its newest matching messages; clicking a chat opens it, clicking a message opens it there.
 */
export const ChatFinder = ({query, onQueryChanged, onSelect}: ChatFinderProps) => {
    const formatTime = useFormatTime()
    const [includeMessages, setIncludeMessagesState] = useState(() => readStorageBool(STORAGE_KEY_MESSAGES, true))
    const [found, setFound] = useState<ChatFindOut[] | null>(null)
    const [loading, setLoading] = useState(false)
    const [failed, setFailed] = useState(false)
    const requestId = useRef(0)

    const setIncludeMessages = (value: boolean) => {
        setIncludeMessagesState(value)
        writeStorageBool(STORAGE_KEY_MESSAGES, value)
    }

    const trimmed = query.trim()

    useEffect(() => {
        if (!trimmed) {
            requestId.current += 1
            setFound(null)
            setLoading(false)
            setFailed(false)
            return
        }

        const id = ++requestId.current
        setLoading(true)
        setFailed(false)

        const timer = setTimeout(async () => {
            try {
                const result = await findChats({query: trimmed, includeMessages})
                if (requestId.current === id) setFound(result.chats)
            } catch {
                if (requestId.current !== id) return
                setFound([])
                setFailed(true)
            } finally {
                if (requestId.current === id) setLoading(false)
            }
        }, DEBOUNCE_MS)

        return () => clearTimeout(timer)
    }, [trimmed, includeMessages])

    return (
        <Div className="vbox chat-search">
            <Input
                autoFocus
                className="chat-search__input"
                placeholder="Search your chats…"
                text={query}
                onChanged={onQueryChanged}
            />

            <Div className="chat-search__filters">
                <label className="chat-search__filter">
                    <Checkbox toggled={includeMessages} onToggled={setIncludeMessages}/>
                    <Label text="Messages"/>
                </label>
            </Div>

            {loading && <Label className="chat-search__status" text="searching…"/>}
            {!loading && failed && <Label className="chat-search__status" text="search failed"/>}
            {!loading && !failed && found && found.length === 0 && (
                <Label className="chat-search__status" text={`no chats match “${trimmed}”`}/>
            )}

            {found && found.length > 0 && (
                <Div className="vbox tree chat-finder__results">
                    {found.map((chat) => (
                        <Div key={chat.chat_id} className="tree__row chat-finder__chat">
                            {/* The name with what matched highlighted; how many messages matched at the right */}
                            <Div className="list-row chat-finder__row" onClick={() => onSelect({chatId: chat.chat_id})}>
                                <span className="chat-finder__name">{chat.name_matched ? highlightText(chat.name, trimmed) : chat.name}</span>
                                {chat.message_matches > 0 ? (
                                    <Label variant="secondary" className="chat-finder__count"
                                           text={`${chat.message_matches} message${chat.message_matches === 1 ? '' : 's'}`}/>
                                ) : null}
                            </Div>
                            {chat.messages.length > 0 ? (
                                <Div className="vbox tree tree--nested">
                                    {chat.messages.map((hit) => (
                                        <Div key={hit.id} className="tree__row">
                                            <Div className="list-row chat-finder__hit"
                                                 onClick={() => onSelect({chatId: chat.chat_id, jump: {hit, query: trimmed}})}>
                                                <Label variant="secondary" className="chat-finder__meta"
                                                       text={`${hit.role} · ${new Date(hit.created_at).toLocaleDateString()}, ${formatTime(hit.created_at)}`}/>
                                                <span className="chat-finder__snippet">
                                                    {hit.before ? <span className="chat-search__dim">…{hit.before}</span> : null}
                                                    <span className="chat-search__match">{hit.matched}</span>
                                                    {hit.after ? <span className="chat-search__dim">{hit.after}…</span> : null}
                                                </span>
                                            </Div>
                                        </Div>
                                    ))}
                                    {chat.message_matches > chat.messages.length ? (
                                        <Div className="tree__row">
                                            <Label variant="secondary" className="chat-finder__more"
                                                   text={`…and ${chat.message_matches - chat.messages.length} more in this chat`}/>
                                        </Div>
                                    ) : null}
                                </Div>
                            ) : null}
                        </Div>
                    ))}
                </Div>
            )}
        </Div>
    )
};
