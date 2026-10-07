import {createContext} from 'react'

export interface RunsContextValue {
    /** Chats to show as working: those with a run going on, and the parent of a sub-agent that works */
    working: ReadonlySet<number>
}

export const RunsContext = createContext<RunsContextValue | null>(null)
