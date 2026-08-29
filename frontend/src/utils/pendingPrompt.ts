const STORAGE_KEY = 'pending_prompt'
const VALID_WINDOW_MS = 2 * 60 * 1000

interface PendingPrompt {
  chatId: number
  prompt: string
  think: boolean
  images: string[]
  expiresAt: number
}

/**
 * Stashes a prompt (and its `think` toggle plus any attached `images`, as set on the
 * home page's composer at send time) so the chat page can auto-send it the moment it
 * lands, right after creating a new chat from the home page's composer —
 * sessionStorage rather than a query param since prompt text (and image data) can be
 * arbitrarily long (query params/URLs have practical length limits; sessionStorage's
 * per-origin quota is megabytes).
 */
export function setPendingPrompt(chatId: number, prompt: string, think: boolean, images: string[] = []): void {
  const value: PendingPrompt = { chatId, prompt, think, images, expiresAt: Date.now() + VALID_WINDOW_MS }
  sessionStorage.setItem(STORAGE_KEY, JSON.stringify(value))
}

function readPendingPrompt(chatId: number): { prompt: string; think: boolean; images: string[] } | null {
  const raw = sessionStorage.getItem(STORAGE_KEY)
  if (!raw) return null

  let parsed: PendingPrompt
  try {
    parsed = JSON.parse(raw)
  } catch {
    return null
  }

  if (parsed.chatId !== chatId) return null
  if (Date.now() > parsed.expiresAt) return null

  return { prompt: parsed.prompt, think: parsed.think, images: parsed.images ?? [] }
}

/**
 * Reads the pending prompt without clearing it, returning it only if it's for `chatId`
 * and still within its time window. For seeding UI state (e.g. the composer's `think`
 * toggle) that needs to already show the right value on first render, before the effect
 * that actually consumes and sends it has had a chance to run.
 */
export function peekPendingPrompt(chatId: number): { prompt: string; think: boolean; images: string[] } | null {
  return readPendingPrompt(chatId)
}

/**
 * Reads and clears the pending prompt, returning it only if it's for `chatId` and still
 * within its time window — guards against a stale or mismatched entry firing on the
 * wrong chat, or long after the tab that stashed it moved on. Always clears the key once
 * read, whether or not it actually matched, so a stale entry can never fire twice or
 * leak into some later unrelated chat.
 */
export function consumePendingPrompt(chatId: number): { prompt: string; think: boolean; images: string[] } | null {
  const result = readPendingPrompt(chatId)
  sessionStorage.removeItem(STORAGE_KEY)
  return result
}
