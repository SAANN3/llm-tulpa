import {useState} from 'react'
import {MoreVertical} from 'pixelarticons/react'
import type {ExternalPlugin} from '../api/plugins/types.ts'
import {Button} from './primitives'
import {ContextMenu, type ContextMenuItem} from './popups/base/context-menu.tsx'
import {UninstallPluginPopup} from './popups/uninstall-plugin-popup.tsx'

export interface ExternalPluginMenuProps {
    plugin: ExternalPlugin
    /** Whether the plugin is in the plugin list (loaded); one that is not can only be loaded again or uninstalled */
    loaded: boolean
    onLoad: () => void
    onUnload: () => void
    onExport: () => void
    onUninstall: (removeData: boolean) => void
}

/** The "more" button of an external plugin: what the owner can do with it besides switching it on and off */
export const ExternalPluginMenu = ({plugin, loaded, onLoad, onUnload, onExport, onUninstall}: ExternalPluginMenuProps) => {
    const [at, setAt] = useState<{ x: number; y: number } | null>(null)
    const [uninstalling, setUninstalling] = useState(false)
    const items: ContextMenuItem[] = [
        loaded ? {label: 'Unload', onSelect: onUnload} : {label: 'Load', onSelect: onLoad},
        {label: 'Export as a .tulpa file', onSelect: onExport},
        {label: 'Uninstall…', onSelect: () => setUninstalling(true), danger: true},
    ]
    return (
        <>
            <Button variant="secondary" className="plugins__gear" title={`${plugin.name}/${plugin.subname}: unload, export, uninstall`}
                    onClicked={(e) => {
                        const rect = e.currentTarget.getBoundingClientRect()
                        setAt({x: rect.left, y: rect.bottom})
                    }}>
                <MoreVertical width={18} height={18}/>
            </Button>
            <ContextMenu position={at} onClose={() => setAt(null)} items={items} minWidth={200}/>
            <UninstallPluginPopup plugin={uninstalling ? `${plugin.name}/${plugin.subname}` : null} onClose={() => setUninstalling(false)} onConfirm={onUninstall}/>
        </>
    )
};
