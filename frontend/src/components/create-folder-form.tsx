import {useState} from 'react'

import '../styles/folder-picker.scss'
import {folderName} from '../api/prompts/folder-name'
import type {FolderOut} from '../api/folders/types'
import {Button, Div, Input, Label} from './primitives'

export interface CreateFolderFormProps {
    createFolder: (name: string) => Promise<FolderOut>
    onCreated: (folder: FolderOut) => void
    onCancel: () => void
}

type CreateMode = 'name' | 'describe'

/** Either type the folder's name directly, or describe what it's for and let the model
 * name it — shared by `FolderPicker`'s inline create and the `/folders` page's own button. */
export const CreateFolderForm = ({createFolder, onCreated, onCancel}: CreateFolderFormProps) => {
    const [mode, setMode] = useState<CreateMode>('name')
    const [draft, setDraft] = useState('')
    const [generating, setGenerating] = useState(false)
    const [error, setError] = useState<string | null>(null)

    const onCreate = async () => {
        const text = draft.trim()
        if (!text) return

        setError(null)
        try {
            let name = text
            if (mode === 'describe') {
                setGenerating(true)
                name = (await folderName(text)).response.trim() || text
            }
            onCreated(await createFolder(name))
        } catch {
            setError('Could not create the folder.')
        } finally {
            setGenerating(false)
        }
    }

    return (
        <Div className="vbox folder-picker__create">
            <Div className="folder-picker__mode">
                <Button variant={mode === 'name' ? undefined : 'secondary'} text="Write name" onClicked={() => setMode('name')}/>
                <Button variant={mode === 'describe' ? undefined : 'secondary'} text="Describe it" onClicked={() => setMode('describe')}/>
            </Div>
            <Input
                autoFocus
                text={draft}
                onChanged={setDraft}
                placeholder={mode === 'name' ? 'Folder name' : "What's this folder for? We'll name it."}
            />
            {error ? <Label variant="secondary" className="folder-picker__error" text={error}/> : null}
            <Div className="folder-picker__create-actions">
                <Button variant="secondary" text="Cancel" onClicked={onCancel}/>
                <Button text={generating ? 'Thinking…' : 'Create'} onClicked={onCreate} disabled={!draft.trim() || generating}/>
            </Div>
        </Div>
    )
};
