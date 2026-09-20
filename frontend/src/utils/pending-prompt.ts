import type {ThinkChoice} from '../api/agent/types'

const STORAGE_KEY = 'pending_prompt'
const VALID_WINDOW_MS = 2 * 60 * 1000

interface PendingPrompt {
    chatId: number
    prompt: string
    think: ThinkChoice
    images: string[]
    fileIds: number[]
    expiresAt: number
}

/** Stashes a prompt so the chat page can auto-send it once the new chat is created */
export const setPendingPrompt = (
    chatId: number,
    prompt: string,
    think: ThinkChoice,
    images: string[] = [],
    fileIds: number[] = [],
): void => {
    const value: PendingPrompt = {chatId, prompt, think, images, fileIds, expiresAt: Date.now() + VALID_WINDOW_MS}
    sessionStorage.setItem(STORAGE_KEY, JSON.stringify(value))
};

const readPendingPrompt = (chatId: number): {
    prompt: string;
    think: ThinkChoice;
    images: string[];
    fileIds: number[]
} | null => {
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

    return {prompt: parsed.prompt, think: parsed.think, images: parsed.images ?? [], fileIds: parsed.fileIds ?? []}
};

/** Reads the pending prompt without clearing it, for seeding UI state on first render */
export const peekPendingPrompt = (chatId: number): {
    prompt: string;
    think: ThinkChoice;
    images: string[];
    fileIds: number[]
} | null => readPendingPrompt(chatId);

/** Reads and clears the pending prompt for a chat */
export const consumePendingPrompt = (chatId: number): {
    prompt: string;
    think: ThinkChoice;
    images: string[];
    fileIds: number[]
} | null => {
    const result = readPendingPrompt(chatId)
    sessionStorage.removeItem(STORAGE_KEY)
    return result
};
