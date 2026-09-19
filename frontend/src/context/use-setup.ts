import {useContext} from 'react'
import {SetupContext} from './setup-context.ts'

export const useSetup = () => {
    const context = useContext(SetupContext)
    if (!context) throw new Error('useSetup must be used within a SetupProvider')
    return context
};
