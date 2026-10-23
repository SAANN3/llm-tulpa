import {useEffect, useState} from 'react'
import {getPluginTab} from '../api/plugins/tab.ts'
import {sendPluginTabAction} from '../api/plugins/tab-action.ts'
import {uploadPluginTabFiles, type PathedFile} from '../api/plugins/tab-upload.ts'
import type {TabLayout} from '../api/plugins/types.ts'
import {errorReason} from '../utils/error-reason.ts'

/** What a plugin's own Settings tab shows the user, and what the user does in it. The layout is read when the tab opens;
 * every action and every upload goes back to the plugin, which answers with the layout as it is after. While one is under
 * way `busy` is set, a failure is `error`, and a text the plugin wants shown in a popup is `popup` until it is closed. */
export const usePluginTabLayout = (pluginName: string, pluginSubname: string) => {
    const [layout, setLayout] = useState<TabLayout | null>(null)
    const [error, setError] = useState<string | null>(null)
    const [busy, setBusy] = useState(false)
    const [popup, setPopup] = useState<TabLayout['popup']>(null)

    useEffect(() => {
        let cancelled = false
        setLayout(null)
        setError(null)
        getPluginTab(pluginName, pluginSubname)
            .then((found) => !cancelled && setLayout(found))
            .catch((e) => !cancelled && setError(errorReason(e, 'Could not load this tab.')))
        return () => {
            cancelled = true
        }
    }, [pluginName, pluginSubname])

    const run = async (request: () => Promise<TabLayout>) => {
        setBusy(true)
        setError(null)
        try {
            const next = await request()
            setLayout(next)
            if (next.popup) setPopup(next.popup)
        } catch (e) {
            setError(errorReason(e, 'That did not work.'))
        } finally {
            setBusy(false)
        }
    }

    return {
        layout,
        error,
        busy,
        popup,
        closePopup: () => setPopup(null),
        /** What a control sends: its action and its arguments */
        act: (action: string, args: Record<string, unknown>) => run(() => sendPluginTabAction(pluginName, pluginSubname, action, args)),
        /** The files dropped on a drop zone, or chosen with a row's button (then `row` is that row) */
        upload: (action: string, files: PathedFile[], row?: string) => run(() => uploadPluginTabFiles(pluginName, pluginSubname, action, files, row)),
    }
}
