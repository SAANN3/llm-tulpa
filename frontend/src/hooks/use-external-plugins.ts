import {useCallback, useEffect, useState} from 'react'
import {exportExternalPlugin} from '../api/plugins/external-export.ts'
import {getExternalPlugins} from '../api/plugins/external-list.ts'
import {loadExternalPlugin} from '../api/plugins/external-load.ts'
import {uninstallExternalPlugin} from '../api/plugins/external-uninstall.ts'
import {unloadExternalPlugin} from '../api/plugins/external-unload.ts'
import type {ExternalPlugin} from '../api/plugins/types.ts'

/** How often the list is read again while a plugin is being restarted, so the page shows it coming back */
const RESTART_POLL_MS = 2000

/** The external plugins and what the owner does with them: load, unload, uninstall, export. The list is read when the page
 * opens and after each of those; `reload` reads it again (after an install, which is not done from here). */
export const useExternalPlugins = () => {
    const [external, setExternal] = useState<ExternalPlugin[]>([])

    const reload = useCallback(async () => {
        try {
            setExternal(await getExternalPlugins())
        } catch {
            // The plugin list itself says when the backend cannot be reached; this one only adds to it
        }
    }, [])

    useEffect(() => {
        void reload()
    }, [reload])

    const restarting = external.some((p) => p.process?.state === 'restarting')
    useEffect(() => {
        if (!restarting) return
        const id = setInterval(() => void reload(), RESTART_POLL_MS)
        return () => clearInterval(id)
    }, [restarting, reload])

    return {
        external,
        reload,
        load: async (p: ExternalPlugin) => {
            await loadExternalPlugin(p.name, p.subname)
            await reload()
        },
        unload: async (p: ExternalPlugin) => {
            await unloadExternalPlugin(p.name, p.subname)
            await reload()
        },
        uninstall: async (p: ExternalPlugin, removeData: boolean) => {
            await uninstallExternalPlugin(p.name, p.subname, removeData)
            await reload()
        },
        exportFile: (p: ExternalPlugin) => exportExternalPlugin(p.name, p.subname, p.version),
    }
};
