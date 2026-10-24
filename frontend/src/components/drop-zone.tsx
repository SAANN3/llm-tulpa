import {useRef, useState} from 'react'
import '../styles/plugin-tab.scss'
import type {PathedFile} from '../api/plugins/tab-upload.ts'
import {readDroppedFiles} from '../utils/read-dropped-files.ts'
import {Button, Div, Label} from './primitives'

export interface DropZoneProps {
    label: string
    /** The file endings it takes (`.zip`, `.md`), all when empty; a dropped folder's own files are not held to it */
    accept: string[]
    busy: boolean
    onFiles: (files: PathedFile[]) => void
    /** Whether a whole folder can be dropped or chosen (the default); a zone for one file turns it off */
    folders?: boolean
    /** Whether several files can be given at once (the default) */
    multiple?: boolean
}

/** Where files, a whole folder or a zip are dropped: or chosen with the two buttons */
export const DropZone = ({label, accept, busy, onFiles, folders = true, multiple = true}: DropZoneProps) => {
    const [over, setOver] = useState(false)
    const filesInput = useRef<HTMLInputElement>(null)
    const folderInput = useRef<HTMLInputElement>(null)
    const takes = (name: string) => accept.length === 0 || accept.some((ending) => name.toLowerCase().endsWith(ending.toLowerCase()))

    const dropped = async (data: DataTransfer) => {
        const files = await readDroppedFiles(data)
        // A file dropped on its own has to be of a kind the zone takes; the files of a dropped folder come with it
        const wanted = files.filter((f) => (folders && f.path.includes('/')) || takes(f.file.name))
        if (wanted.length > 0) onFiles(multiple ? wanted : wanted.slice(0, 1))
    }

    return (
        <Div className={`drop-zone${over ? ' drop-zone--over' : ''}`}
             onDragOver={(e) => {
                 e.preventDefault()
                 setOver(true)
             }}
             onDragLeave={() => setOver(false)}
             onDrop={(e) => {
                 e.preventDefault()
                 setOver(false)
                 if (!busy) void dropped(e.dataTransfer)
             }}>
            <Label text={busy ? 'Working…' : label}/>
            <Div className="drop-zone__buttons">
                <Button variant="secondary" text={multiple ? 'Choose files' : 'Choose a file'} disabled={busy} onClicked={() => filesInput.current?.click()}/>
                {folders ? <Button variant="secondary" text="Choose a folder" disabled={busy} onClicked={() => folderInput.current?.click()}/> : null}
            </Div>
            <input ref={filesInput} type="file" multiple={multiple} hidden accept={accept.join(',')}
                   onChange={(e) => {
                       const picked = Array.from(e.target.files ?? []).map((file) => ({path: file.name, file}))
                       e.target.value = ''
                       if (picked.length > 0) onFiles(picked)
                   }}/>
            <input ref={folderInput} type="file" hidden
                   // The attribute is not in the typings; it makes the picker take a folder and give each file's path
                   {...({webkitdirectory: '', directory: ''} as object)}
                   onChange={(e) => {
                       const picked = Array.from(e.target.files ?? []).map((file) => ({path: file.webkitRelativePath || file.name, file}))
                       e.target.value = ''
                       if (picked.length > 0) onFiles(picked)
                   }}/>
        </Div>
    )
};
