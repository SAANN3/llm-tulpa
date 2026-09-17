export interface ChatOut {
    id: number
    name: string
    created_at: string
    updated_at: string
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
}

export interface MessagesResponse {
    messages: MessageOut[]
    total: number
}
