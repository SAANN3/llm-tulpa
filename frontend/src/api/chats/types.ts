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

/** `id` present → a single chat's full info. `id` absent → the paginated chat list. */
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
  /** The model's reasoning trace for this reply, when `think` was requested and the model produced one. */
  thinking: string | null
  /** How long the Ollama call for this reply took, in milliseconds. */
  thought_duration_ms: number | null
  tool_calls: MessageToolCallOut[]
  /** Base64-encoded image data (no data-URL prefix) attached to this message, if any. Empty for every role but `user`. */
  images: string[]
}

export interface MessagesResponse {
  messages: MessageOut[]
  total: number
}
