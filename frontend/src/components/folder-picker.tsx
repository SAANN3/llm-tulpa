import {useState} from 'react'

import '../styles/folder-picker.scss'
import {useFolders} from '../hooks/use-folders.ts'
import {CreateFolderForm} from './create-folder-form.tsx'
import {Button, Div, Label} from './primitives'

export interface FolderPickerProps {
    selected: number | null
    onSelect: (folderId: number | null) => void
}

/** Assign a chat to a folder: pick an existing one, clear it, or create a new one — either
 * by typing the name directly, or describing the folder and letting the model name it. */
export const FolderPicker = ({selected, onSelect}: FolderPickerProps) => {
    const {folders, total, loadOlder, createFolder} = useFolders()
    const [creating, setCreating] = useState(false)

    return (
        <Div className="folder-picker">
            <Div className="folder-picker__list">
                <Div
                    variant={selected == null ? 'primary' : undefined}
                    className="list-row folder-picker__row folder-picker__row--none"
                    onClick={() => onSelect(null)}
                >
                    <Label variant={selected == null ? undefined : 'secondary'} text="— No folder —"/>
                </Div>
                {folders.map((f) => (
                    <Div
                        key={f.id}
                        variant={selected === f.id ? 'primary' : undefined}
                        className="list-row folder-picker__row"
                        onClick={() => onSelect(f.id)}
                    >
                        <Label text={f.name}/>
                    </Div>
                ))}
                {folders.length < total ? (
                    <Button variant="secondary" className="folder-picker__load-more" text="Load more" onClicked={loadOlder}/>
                ) : null}
            </Div>

            {creating ? (
                <CreateFolderForm
                    createFolder={createFolder}
                    onCreated={(folder) => {
                        onSelect(folder.id)
                        setCreating(false)
                    }}
                    onCancel={() => setCreating(false)}
                />
            ) : (
                <Button variant="secondary" text="+ New folder" onClicked={() => setCreating(true)}/>
            )}
        </Div>
    )
};
