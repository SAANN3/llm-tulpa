/**
 * Per-chat drafts for the composer: the text and attachments the user has typed
 * but not sent yet, kept in sessionStorage so that switching between chats (or
 * pages) doesn't lose them. Keyed by chat id, or a fixed string for the new-chat
 * (home) page.
 */

export interface Draft {
    text: string
    images: string[]
    fileIds: number[]
}

const STORAGE_PREFIX = 'chat_draft_'

const textKey = (key: number | string): string => `${STORAGE_PREFIX}${key}`
const imagesKey = (key: number | string): string => `${STORAGE_PREFIX}${key}_images`

// The images array last written for a key. Typing changes the text only, and the same array
// reference means the base64 payload is not serialized again on every keystroke.
const imagesWritten = new Map<string, string[]>()

const strings = (value: unknown): string[] =>
    Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : []

const numbers = (value: unknown): number[] =>
    Array.isArray(value) ? value.filter((item): item is number => typeof item === 'number') : []

/** Reads the saved draft for a key, or null if there is none (or it is unreadable). */
export const getDraft = (key: number | string): Draft | null => {
    const raw = sessionStorage.getItem(textKey(key))
    if (!raw) return null
    try {
        const parsed = JSON.parse(raw)
        const text = typeof parsed.text === 'string' ? parsed.text : ''
        const fileIds = numbers(parsed.fileIds)
        // Images live in their own entry. A draft written before that split keeps them inline.
        let images = strings(parsed.images)
        const storedImages = sessionStorage.getItem(imagesKey(key))
        if (storedImages) {
            try {
                images = strings(JSON.parse(storedImages))
            } catch {
                images = []
            }
        }
        if (text === '' && images.length === 0 && fileIds.length === 0) return null
        return {text, images, fileIds}
    } catch {
        return null
    }
}

/**
 * Saves a draft. Text and uploaded file ids are small and written on every change.
 * Base64 images are written only when that array changes, and dropped if they
 * don't fit, so a picture never costs the text.
 */
export const setDraft = (key: number | string, draft: Draft): void => {
    const storageKey = textKey(key)
    try {
        sessionStorage.setItem(storageKey, JSON.stringify({text: draft.text, fileIds: draft.fileIds}))
    } catch {
        // The text alone doesn't fit; give up quietly
        return
    }
    const imageStorageKey = imagesKey(key)
    if (imagesWritten.get(imageStorageKey) === draft.images) return
    if (draft.images.length === 0) {
        sessionStorage.removeItem(imageStorageKey)
        imagesWritten.set(imageStorageKey, draft.images)
        return
    }
    try {
        sessionStorage.setItem(imageStorageKey, JSON.stringify(draft.images))
    } catch {
        sessionStorage.removeItem(imageStorageKey)
    }
    imagesWritten.set(imageStorageKey, draft.images)
}

/** Removes the saved draft for a key (the message was sent, or the chat was deleted). */
export const clearDraft = (key: number | string): void => {
    sessionStorage.removeItem(textKey(key))
    const imageStorageKey = imagesKey(key)
    sessionStorage.removeItem(imageStorageKey)
    imagesWritten.delete(imageStorageKey)
}
