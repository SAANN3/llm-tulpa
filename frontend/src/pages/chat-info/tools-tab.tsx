import {useEffect, useState, type RefObject} from 'react'
import {getAvailableTools, type AvailableTool} from '../../api/chats/available-tools.ts'
import {getChatTools} from '../../api/chats/get-tools.ts'
import {setChatTools} from '../../api/chats/set-tools.ts'
import {getToolChoice} from '../../api/settings/tools-get.ts'
import type {ToolMode} from '../../api/settings/types.ts'
import {ToolChoice} from '../../components/tool-choice.tsx'
import {Button, Div, Label} from '../../components/primitives'

const sameNames = (a: string[], b: string[]): boolean => a.length === b.length && [...a].sort().every((name, i) => name === [...b].sort()[i])

/** Which tools this chat sends. The page's Save writes it. Reset copies the user's default from Settings into the draft. */
export const ToolsTab = ({chatId, readOnly, dirtyRef, onDirty, onStatus}: {
    chatId: number
    readOnly: boolean
    /** The page's Save calls this while the tab is open */
    dirtyRef: RefObject<(() => Promise<void>) | null>
    onDirty: (dirty: boolean) => void
    onStatus: (status: string | null) => void
}) => {
    const [catalog, setCatalog] = useState<AvailableTool[] | null>(null)
    const [savedMode, setSavedMode] = useState<ToolMode>('blacklist')
    const [savedNames, setSavedNames] = useState<string[]>([])
    const [mode, setMode] = useState<ToolMode>('blacklist')
    const [names, setNames] = useState<string[]>([])
    const [failed, setFailed] = useState(false)

    useEffect(() => {
        let cancelled = false
        Promise.all([getChatTools(chatId), getAvailableTools()])
            .then(([choice, tools]) => {
                if (cancelled) return
                setCatalog(tools)
                setSavedMode(choice.mode)
                setSavedNames(choice.tools)
                setMode(choice.mode)
                setNames(choice.tools)
            })
            .catch(() => {
                if (!cancelled) setFailed(true)
            })
        return () => {
            cancelled = true
            dirtyRef.current = null
        }
    }, [chatId, dirtyRef])

    const dirty = mode !== savedMode || !sameNames(names, savedNames)
    useEffect(() => onDirty(catalog != null && dirty), [catalog, dirty, onDirty])

    // Assigned each render so Save writes the switches as they are now
    dirtyRef.current = async () => {
        await setChatTools(chatId, {mode, tools: names})
        setSavedMode(mode)
        setSavedNames(names)
    }

    if (!catalog) return <Label variant="secondary" text={failed ? 'Could not load the tools.' : 'Loading…'}/>

    return (
        <Div className="vbox chat-info__stack">
            <Div className="tool-settings__actions">
                <Button variant="secondary" text="Reset to default" disabled={readOnly} onClicked={() => {
                    onStatus(null)
                    getToolChoice()
                        .then((choice) => {
                            setMode(choice.mode)
                            setNames(choice.tools)
                        })
                        .catch(() => onStatus('Could not load the default.'))
                }}/>
            </Div>
            <ToolChoice catalog={catalog} mode={mode} names={names} disabled={readOnly} onMode={setMode} onNames={setNames}/>
        </Div>
    )
}
