/** What an entry is about: who you are, how you like things done, or anything else lasting */
export type MemoryCategory = 'about_you' | 'preferences' | 'misc'

export interface MemoryEntry {
    id: number
    category: MemoryCategory
    text: string
    /** Your own words a learned entry rests on; null for one you wrote or edited */
    quote: string | null
    /** Written by the model, which may correct it and drops the oldest of these for your limit */
    learned: boolean
    updated_at: string
}

export interface MemoryResponse {
    /** The most recently changed first */
    entries: MemoryEntry[]
}

export interface MemoryEntryRequest {
    category: MemoryCategory
    text: string
}

export interface UpdateMemoryRequest extends MemoryEntryRequest {
    id: number
}
