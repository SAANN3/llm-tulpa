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
}

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
