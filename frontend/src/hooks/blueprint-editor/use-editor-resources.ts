import {useEffect, useState} from 'react'
import {getAvailableTools, type AvailableTool} from '../../api/chats/available-tools'
import {useProfileCatalog} from '../use-profile-catalog'

/** Tools and registered models the node forms share. A failed load leaves the lists empty and says so. */
export const useEditorResources = () => {
    const {models, profiles} = useProfileCatalog()
    const [tools, setTools] = useState<AvailableTool[]>([])
    const [toolsLoaded, setToolsLoaded] = useState(false)
    const [toolsError, setToolsError] = useState<string | null>(null)
    useEffect(() => {
        let cancel = false
        getAvailableTools().then(next => {
            if (cancel) return
            setTools(next)
            setToolsLoaded(true)
        }).catch(() => {
            if (cancel) return
            setToolsError('Tools could not be loaded. Saved tool settings are unchanged.')
            setToolsLoaded(true)
        })
        return () => { cancel = true }
    }, [])
    return {tools, toolsLoaded, toolsError, models, profiles}
}
