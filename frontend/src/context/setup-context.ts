import {createContext} from 'react'
import type {SetupStatus} from '../api/setup/types'

export interface SetupContextValue {
    status: SetupStatus | null
    loading: boolean
    /** Why the backend couldn't be reached (no answer, or an error answer to the status call), null while it answers */
    unreachable: string | null
    refresh: () => Promise<void>
}

export const SetupContext = createContext<SetupContextValue | null>(null)
