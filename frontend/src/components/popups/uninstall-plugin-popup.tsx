import {useState} from 'react'
import '../../styles/uninstall-plugin-popup.scss'
import {Checkbox, Div, Label} from '../primitives'
import {Popup} from './base/popup.tsx'
import {PopupActions} from './base/popup-actions.tsx'

export interface UninstallPluginPopupProps {
    /** The plugin's name, `name/subname`; the popup is open while there is one */
    plugin: string | null
    /** Runs after the popup has closed itself */
    onConfirm: (removeData: boolean) => void
    onClose: () => void
}

// Its own component so the choice starts unticked every time the popup opens
const UninstallForm = ({plugin, onConfirm, onClose}: { plugin: string; onConfirm: (removeData: boolean) => void; onClose: () => void }) => {
    const [removeData, setRemoveData] = useState(false)
    const confirm = () => {
        onClose()
        onConfirm(removeData)
    }
    return (
        <>
            <Label text={`Uninstall ${plugin}? Its program is deleted from this machine.`}/>
            <Div className="uninstall-plugin__choice">
                <Checkbox toggled={removeData} onToggled={setRemoveData}/>
                <Label text="Also delete what it kept: its settings, its database tables and its files"/>
            </Div>
            <Label variant="secondary" text={removeData ? 'This cannot be undone.' : 'They stay, and installing the plugin again finds them.'}/>
            <PopupActions confirmLabel="Uninstall" onConfirm={confirm} onCancel={onClose}/>
        </>
    )
}

/** Asks before an external plugin is removed, and whether what it stored goes too */
export const UninstallPluginPopup = ({plugin, onConfirm, onClose}: UninstallPluginPopupProps) => (
    <Popup open={plugin !== null} onClose={onClose} title="Uninstall" actions={[]}>
        {plugin ? <UninstallForm plugin={plugin} onConfirm={onConfirm} onClose={onClose}/> : null}
    </Popup>
);
