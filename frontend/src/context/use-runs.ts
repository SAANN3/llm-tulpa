import {useContext} from 'react'
import {RunsContext, type RunsContextValue} from './runs-context.ts'

/** Which chats are working */
export const useRuns = (): RunsContextValue => {
    const context = useContext(RunsContext)

    if (!context) {
        throw new Error('useRuns must be used within a RunsProvider')
    }

    return context
};
