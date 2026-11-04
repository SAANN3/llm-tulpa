import {useEffect, useState} from 'react'
import '../../styles/tool-settings.scss'
import {getAvailableTools, type AvailableTool} from '../../api/chats/available-tools.ts'
import {getToolChoice} from '../../api/settings/tools-get.ts'
import {setToolChoice} from '../../api/settings/tools-set.ts'
import type {ToolMode} from '../../api/settings/types.ts'
import {ToolChoice} from '../../components/tool-choice.tsx'
import {Button, Div, Label} from '../../components/primitives'
import {useDocumentTitle} from '../../hooks/use-document-title.ts'
import {useGoBack} from '../../hooks/use-go-back.ts'

const sameNames = (a: string[], b: string[]): boolean => a.length === b.length && [...a].sort().every((name, i) => name === [...b].sort()[i])

/** Which tools a new chat sends. A chat can choose its own afterwards. */
const ToolsSettings = () => {
    useDocumentTitle('Tools')
    const goBack = useGoBack('/settings/behaviour')
    const [loading, setLoading] = useState(true)
    const [catalog, setCatalog] = useState<AvailableTool[]>([])
    const [savedMode, setSavedMode] = useState<ToolMode>('blacklist')
    const [savedNames, setSavedNames] = useState<string[]>([])
    const [mode, setMode] = useState<ToolMode>('blacklist')
    const [names, setNames] = useState<string[]>([])
    const [saving, setSaving] = useState(false)
    const [error, setError] = useState<string | null>(null)

    useEffect(() => {
        let cancelled = false
        setLoading(true)
        Promise.all([getToolChoice(), getAvailableTools()])
            .then(([choice, tools]) => {
                if (cancelled) return
                setCatalog(tools)
                setSavedMode(choice.mode)
                setSavedNames(choice.tools)
                setMode(choice.mode)
                setNames(choice.tools)
            })
            .catch(() => !cancelled && setError('Could not load the tools.'))
            .finally(() => !cancelled && setLoading(false))
        return () => {
            cancelled = true
        }
    }, [])

    if (loading) return null

    const dirty = mode !== savedMode || !sameNames(names, savedNames)

    const save = async (nextMode: ToolMode, nextNames: string[]) => {
        setSaving(true)
        setError(null)
        try {
            await setToolChoice({mode: nextMode, tools: nextNames})
            setSavedMode(nextMode)
            setSavedNames(nextNames)
            setMode(nextMode)
            setNames(nextNames)
        } catch {
            setError('Saving failed.')
        } finally {
            setSaving(false)
        }
    }

    const onReset = () => void save('blacklist', [])

    return (
        <Div className="page vbox tool-settings">
            <Div className="tool-settings__header">
                <Div className="tool-settings__header-bar">
                    <Button className="tool-settings__back" variant="secondary" text="Back" onClicked={goBack}/>
                    <Label className="section-heading" text="Tools in new chats"/>
                    <Label variant="secondary" className="tool-settings__status"
                           text={error ?? (saving ? 'Saving…' : savedMode === 'blacklist' && savedNames.length === 0 ? 'Every tool is sent' : 'A selection is saved')}/>
                </Div>
                <Div className="tool-settings__spacer"/>
                <Div className="tool-settings__actions">
                    <Button className="tool-settings__action" variant="secondary" text="Reset to default" disabled={saving} onClicked={onReset}/>
                    <Button className="tool-settings__action" text="Save" disabled={saving || !dirty} onClicked={() => void save(mode, names)}/>
                </Div>
            </Div>
            <ToolChoice catalog={catalog} mode={mode} names={names} disabled={saving} onMode={setMode} onNames={setNames}/>
        </Div>
    )
}

export default ToolsSettings
