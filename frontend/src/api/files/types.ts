/** A file `FileStore` manages — mirrors the backend's `files` table. */
export interface FileOut {
  id: number
  /** `null` for a file uploaded before a chat existed to attach it to yet (e.g. from the home page's composer, before that first message has actually been sent) — claimed the moment it's actually attached to a message. */
  chat_id: number | null
  /** Where this file actually lives on disk — a generated name, not anything a user picked. */
  full_path: string
  /** The user/UI-facing name for this file (e.g. what it was uploaded as). */
  file_name: string
  /** Whether this file should be treated as immutable. Defaults to `true` on upload. */
  read_only: boolean
}
