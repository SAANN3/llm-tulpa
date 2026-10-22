import type {MemoryCategory} from '../api/memory/types.ts'

/** The three kinds of entry in the user's memory, in the order they are listed: how they like things done matters most to the model */
export const MEMORY_CATEGORIES: { value: MemoryCategory; label: string; help: string }[] = [
    {value: 'preferences', label: 'How you like things done', help: 'style, language, habits'},
    {value: 'about_you', label: 'About you', help: 'who you are, your setup'},
    {value: 'misc', label: 'Other', help: 'anything else lasting'},
]
