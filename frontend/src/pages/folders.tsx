import {useState} from 'react'
import {useNavigate} from 'react-router-dom'

import '../styles/folders.scss'
import {CreateFolderForm} from '../components/create-folder-form.tsx'
import {FolderEntry} from '../components/folder-entry.tsx'
import {LazyList} from '../components/lazy-list.tsx'
import {Popup} from '../components/popup.tsx'
import {Button, Div, Input, Label} from '../components/primitives'
import {Sidebar} from '../components/sidebar.tsx'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {useFolders} from '../hooks/use-folders.ts'

/** The full-page folder list: every folder the user owns, ordered by their most recently
 * active chat, with a name search — the destination of the sidebar's "Folders" util entry. */
const Folders = () => {
    useDocumentTitle('Folders')
    const navigate = useNavigate()
    const [query, setQuery] = useState('')
    const [creating, setCreating] = useState(false)
    const {folders, loadOlder, createFolder, rename, delete: deleteFolder} = useFolders(query)

    return (
        <Div className="page">
            <Sidebar/>
            <Div className="vbox folders">
                <Div className="folders__header">
                    <TypewriterLabel className="mono folders__title" text="~/folders" charIntervalMs={30}/>
                    <Button variant="secondary" text="+ New folder" onClicked={() => setCreating(true)}/>
                </Div>
                <Input className="folders__search" text={query} onChanged={setQuery} placeholder="Search folders"/>
                <LazyList onBottomReached={loadOlder} className="folders__list">
                    <Div className="vbox">
                        {folders.map((f) => (
                            <Div key={f.id} className="folders__tree-row">
                                <FolderEntry
                                    label={f.name}
                                    selected={false}
                                    onClicked={() => navigate(`/folders/${f.id}`)}
                                    onRename={(name) => rename(f.id, name)}
                                    onDelete={() => deleteFolder(f.id)}
                                />
                            </Div>
                        ))}
                        {folders.length === 0 ? (
                            <Label variant="secondary" className="folders__empty" text="No folders yet"/>
                        ) : null}
                    </Div>
                </LazyList>
            </Div>

            <Popup open={creating} onClose={() => setCreating(false)} centered>
                <Div className="dos-frame sidebar__folder-picker">
                    <span className="dos-frame__title">New folder</span>
                    <Div className="dos-frame__body">
                        <CreateFolderForm
                            createFolder={createFolder}
                            onCreated={(folder) => {
                                setCreating(false)
                                navigate(`/folders/${folder.id}`)
                            }}
                            onCancel={() => setCreating(false)}
                        />
                    </Div>
                </Div>
            </Popup>
        </Div>
    )
};

export default Folders
