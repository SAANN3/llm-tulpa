import {useEffect, useState} from 'react'
import {getAvailableTools, type AvailableTool} from '../../api/chats/available-tools.ts'
import {getToolChoice} from '../../api/settings/tools-get.ts'
import type {ToolChoice, ToolMode} from '../../api/settings/types.ts'
import {ToolChoice as ToolChoiceList} from '../tool-choice.tsx'
import {Button, Div, Label} from '../primitives'
import {Popup} from './base/popup.tsx'
import {PopupActions} from './base/popup-actions.tsx'

/** The tools a chat that does not exist yet will send. Done keeps the draft for that chat only. */
export const ChatToolsPopup = ({open, initial, onClose, onUse, onUseDefault}: {
    open: boolean
    /** The choice already made for this new chat, or null to start from the user's default */
    initial: ToolChoice | null
    onClose: () => void
    onUse: (choice: ToolChoice) => void
    onUseDefault: () => void
}) => {
    const [catalog, setCatalog] = useState<AvailableTool[] | null>(null)
    const [mode, setMode] = useState<ToolMode>('blacklist')
    const [names, setNames] = useState<string[]>([])
    const [failed, setFailed] = useState(false)

    useEffect(() => {
        if (!open) return
        let cancelled = false
        setCatalog(null)
        setFailed(false)
        const choice = initial ? Promise.resolve(initial) : getToolChoice()
        Promise.all([choice, getAvailableTools()])
            .then(([picked, tools]) => {
                if (cancelled) return
                setCatalog(tools)
                setMode(picked.mode)
                setNames(picked.tools)
            })
            .catch(() => {
                if (!cancelled) setFailed(true)
            })
        return () => {
            cancelled = true
        }
    }, [open, initial])

    return (
        <Popup open={open} onClose={onClose} title="Tools for this chat" width={640} actions={[]}>
            {catalog ? (
                <Div className="vbox" style={{gap: 12, minHeight: 0}}>
                    <ToolChoiceList catalog={catalog} mode={mode} names={names} onMode={setMode} onNames={setNames}/>
                    <Div className="popup__actions popup__actions--stacked">
                        <Button variant="secondary" text="Use default" onClicked={onUseDefault}/>
                    </Div>
                    <PopupActions emphasis="confirm" confirmLabel="Apply" onConfirm={() => onUse({mode, tools: names})} onCancel={onClose}/>
                </Div>
            ) : (
                <Label variant="secondary" text={failed ? 'Could not load the tools.' : 'Loading…'}/>
            )}
        </Popup>
    )
}
