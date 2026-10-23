import {useEffect, useState} from 'react'
import {getPluginTabs} from '../api/plugins/tabs.ts'
import type {PluginTabInfo} from '../api/plugins/types.ts'

/** The Settings tabs the enabled plugins declare, read when the page opens, and whether that has happened yet */
export const usePluginTabs = (): { tabs: PluginTabInfo[]; loaded: boolean } => {
    const [tabs, setTabs] = useState<PluginTabInfo[]>([])
    const [loaded, setLoaded] = useState(false)
    useEffect(() => {
        let cancelled = false
        getPluginTabs()
            .then((found) => !cancelled && setTabs(found))
            .catch(() => !cancelled && setTabs([]))
            .finally(() => !cancelled && setLoaded(true))
        return () => {
            cancelled = true
        }
    }, [])
    return {tabs, loaded}
}
