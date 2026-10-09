import {useState} from 'react'

import '../styles/folder-picker.scss'
import {folderName} from '../api/prompts/folder-name'
import type {FolderOut} from '../api/folders/types'
import {ChoiceGroup, Div, Input, Label} from './primitives'
import {PopupActions} from './popups/base/popup-actions.tsx'

export interface CreateFolderFormProps {
    createFolder: (name: string) => Promise<FolderOut>
    onCreated: (folder: FolderOut) => void
    onCancel: () => void
}

type CreateMode = 'describe' | 'name'

/** Either type the folder's name directly, or describe what it's for and let the model
 * name it — shared by `FolderPicker`'s inline create and the `/folders` page's own button. */
export const CreateFolderForm = ({createFolder, onCreated, onCancel}: CreateFolderFormProps) => {
    const [mode, setMode] = useState<CreateMode>('describe')
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
            <ChoiceGroup className="folder-picker__mode" label="How to name the folder" chosen={mode}
                         onChosen={(chosen) => setMode(chosen as CreateMode)}
                         options={[{value: 'describe', label: 'Describe it'}, {value: 'name', label: 'Write a name'}]}/>
            <Input
                autoFocus
                text={draft}
                onChanged={setDraft}
                placeholder={mode === 'describe' ? 'What is this folder for?' : 'Folder name'}
                onKeyDown={(e) => e.key === 'Enter' && void onCreate()}
            />
            {error
                ? <Label variant="secondary" className="folder-picker__error" text={error}/>
                : mode === 'describe' ? <Label variant="secondary" className="field__help" text="The model names it from your description."/> : null}
            <PopupActions
                emphasis="confirm"
                confirmLabel={generating ? 'Thinking…' : 'Create'}
                confirmDisabled={!draft.trim() || generating}
                onConfirm={onCreate}
                onCancel={onCancel}
            />
        </Div>
    )
};
