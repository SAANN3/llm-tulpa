export interface ChatOut {
    id: number
    name: string
    model: string
    provider: string
    created_at: string
    updated_at: string
    /** Ollama's measured prompt size (prompt + generated tokens) at the end of the chat's last model call — the current context usage. Null until the first turn with metrics. */
    last_prompt_tokens: number | null
    /** The context window the agent runs under — the max for a context usage display. */
    context_length: number
    /** The folder this chat is grouped under, or null if ungrouped. */
    folder_id: number | null
    /** The chat that started this one as a sub-agent, or null for an ordinary chat. */
    parent_chat_id: number | null
    /** The launch profile this chat runs its model under, or null for a model with none (an Ollama model). */
    launch_profile_id: number | null
    /** Whether the model is sent its tools in this chat. */
    tools_enabled: boolean
    /** How the chat's last run ended, until the chat is opened; null when there is nothing new. */
    unseen_end: UnseenEnd | null
}

/** The ways a run ends that the user hasn't looked at yet (a run they stopped leaves none) */
export type UnseenEnd = 'answered' | 'waiting_for_permission' | 'failed' | 'step_limit'

export interface ChatListOut {
    chats: ChatOut[]
    total: number
}

export type GetChatsResponse = ChatOut | ChatListOut

export interface MessageToolCallOut {
    tool_name: string
    arguments: Record<string, unknown>
}

export interface MessageOut {
    id: number
    chat_id: number
    role: string
    content: string
    tool_name: string | null
    created_at: string
    thinking: string | null
    thought_duration_ms: number | null
    tool_calls: MessageToolCallOut[]
    images: string[]
    file_ids: number[]
    prompt_tokens?: number | null
    eval_tokens?: number | null
}

export interface MessagesResponse {
    messages: MessageOut[]
    total: number
}

export interface MessageSearchOut {
    id: number
    role: string
    created_at: string
    /** The content before the matched text, trimmed to a snippet; null when the match starts at the message's beginning */
    before: string | null
    /** The matched text itself, as it appears in the message */
    matched: string
    /** The content after the matched text, trimmed to a snippet; null when the match ends at the message's end */
    after: string | null
    /** Where the match was found: "content", "thinking", or "arguments" (a tool call's) */
    matched_in: string
}

export interface MessageSearchResponse {
    matches: MessageSearchOut[]
    total: number
}

/** One chat of a `find` result: why it matched, and the newest messages that did */
export interface ChatFindOut {
    chat_id: number
    name: string
    updated_at: string
    /** Whether the chat's name contains the text */
    name_matched: boolean
    /** How many of the chat's messages contain it */
    message_matches: number
    /** The newest matching messages, a few of them; shaped like an in-chat search hit */
    messages: MessageSearchOut[]
}

export interface FindChatsResponse {
    /** Most recently active first */
    chats: ChatFindOut[]
}

export interface RecentModelOut {
    provider: string
    model: string
    /** The name the owner gave the model, when it has one */
    display_name: string | null
    /** The launch profile the chat runs the model under; null for an Ollama model */
    launch_profile_id: number | null
}

export interface RecentModelsResponse {
    models: RecentModelOut[]
}

/** One part of what fills a chat's context, in the order the prompt sends them */
export type ContextPartKind =
    | 'system_prompt'
    | 'tools'
    | 'key_facts'
    | 'pinned'
    | 'summary'
    | 'notes'
    | 'user_messages'
    | 'replies'
    | 'thinking'
    | 'tool_calls'
    | 'tool_results'

export interface ContextPartOut {
    kind: ContextPartKind
    tokens: number
    chars: number
    /** How many there are of it (tools, facts, messages, calls); null for one block of text */
    count: number | null
}

/** What the chat remembers past a fold */
export interface ChatMemoryOut {
    /** Set by the summarizer at the first fold; not editable */
    goal: string | null
    facts: string[]
    summary: string | null
    /** The newest notes: the model's newer ones still waiting for the next fold, when there are any */
    notes: string | null
    notes_pending: boolean
    /** Before the first fold there are no facts or summary to edit */
    folded: boolean
}

export interface ChatContextResponse {
    context_length: number
    /** The prompt size at which the chat is folded */
    fold_at: number
    /** True: `used` is the model server's count of the last prompt, shared out over the parts. False: all estimates. */
    measured: boolean
    used: number
    parts: ContextPartOut[]
    cleared_results: number
    cleared_tokens: number
    messages: number
    folded_messages: number
    generated_tokens: number
    subagent_chats: number
    memory: ChatMemoryOut
}
