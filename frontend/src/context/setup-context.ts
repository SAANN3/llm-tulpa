import {createContext} from 'react'
import type {SetupStatus} from '../api/setup/types'

export interface SetupContextValue {
    status: SetupStatus | null
    loading: boolean
    refresh: () => Promise<void>
}

export const SetupContext = createContext<SetupContextValue | null>(null)
