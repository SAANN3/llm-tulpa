import {useEffect, useState} from 'react'
import {ArrowLeft} from 'pixelarticons/react'
import {useNavigate, useParams} from 'react-router-dom'

import '../styles/folders.scss'
import {deleteFolder} from '../api/folders/delete'
import {getFolders} from '../api/folders/get'
import {renameFolder} from '../api/folders/rename'
import {setChatFolder} from '../api/chats/set-folder'
import type {ChatOut} from '../api/chats/types'
import {ChatEntry} from '../components/chat-entry.tsx'
import {CreateChatForm} from '../components/create-chat-form.tsx'
import {FolderPicker} from '../components/folder-picker.tsx'
import {LazyList} from '../components/lazy-list.tsx'
import {Popup} from '../components/popup.tsx'
import {Button, Div, Input, Label} from '../components/primitives'
import {Sidebar} from '../components/sidebar.tsx'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useChats} from '../hooks/use-chats.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'

/** One folder's own page: its chats, newest-active first, plus rename/delete for the
 * folder itself — the destination of a folder row on `/folders` and of a chat header's
 * "jump to folder" button. */
const Folder = () => {
    const {id} = useParams<{ id: string }>()
    const folderId = Number(id)
    const navigate = useNavigate()
    const [folderName, setFolderName] = useState<string | null>(null)
    const [renaming, setRenaming] = useState(false)
    const [renameDraft, setRenameDraft] = useState('')
    const [confirmingDelete, setConfirmingDelete] = useState(false)
    const [assigningChat, setAssigningChat] = useState<ChatOut | null>(null)
    const [creatingChat, setCreatingChat] = useState(false)
    useDocumentTitle(folderName ?? 'Folder')

    useEffect(() => {
        setFolderName(null)
        let cancelled = false
        getFolders({id: folderId}).then((result) => {
            if (!cancelled && !('folders' in result)) setFolderName(result.name)
        })
        return () => {
            cancelled = true
        }
    }, [folderId])

    const {chats, loadOlder, rename, delete: deleteChat, createChat} = useChats(undefined, folderId)

    const onRename = async () => {
        const name = renameDraft.trim()
        if (!name) return
        await renameFolder(folderId, name)
        setFolderName(name)
        setRenaming(false)
    }

    const onDelete = async () => {
        setConfirmingDelete(false)
        // Doesn't delete its chats — `chats.folder_id`'s `ON DELETE SET NULL` just ungroups them.
        await deleteFolder(folderId)
        navigate('/folders')
    }

    const onCreateChat = () => {
        setCreatingChat(true)
    }

    const onChatCreated = async (chat: ChatOut) => {
        await setChatFolder(chat.id, folderId)
        navigate(`/chat?id=${chat.id}`)
    }

    return (
        <Div className="page">
            <Sidebar/>
            <Div className="vbox folders">
                <Div className="folders__header">
                    <Button variant="secondary" className="folders__back" onClicked={() => navigate('/folders')}>
                        <ArrowLeft width={16} height={16}/>
                    </Button>
                    {folderName != null ? (
                        <TypewriterLabel className="mono folders__title" text={`~/folders/${folderName}`} charIntervalMs={30}/>
                    ) : (
                        <Label className="mono folders__title" text="~/folders/…"/>
                    )}
                    <Button variant="secondary" className="folders__new-chat" onClicked={onCreateChat} text="New chat"/>
                    <Button
                        variant="secondary"
                        text="Rename"
                        onClicked={() => {
                            setRenameDraft(folderName ?? '')
                            setRenaming(true)
                        }}
                        disabled={folderName == null}
                    />
                    <Button
                        variant="secondary"
                        text="Delete"
                        onClicked={() => setConfirmingDelete(true)}
                        disabled={folderName == null}
                    />
                </Div>
                <LazyList onBottomReached={loadOlder} className="folders__list">
                    <Div className="vbox">
                        {chats.map((c) => (
                            <Div key={c.id} className="folders__tree-row">
                                <ChatEntry
                                    label={c.name}
                                    selected={false}
                                    onClicked={() => navigate(`/chat?id=${c.id}`)}
                                    onRename={(name) => rename(c.id, name)}
                                    onDelete={() => deleteChat(c.id)}
                                    onAssignFolder={() => setAssigningChat(c)}
                                />
                            </Div>
                        ))}
                        {chats.length === 0 ? (
                            <Label variant="secondary" className="folders__empty" text="No chats in this folder yet"/>
                        ) : null}
                    </Div>
                </LazyList>
            </Div>

            <Popup open={renaming} onClose={() => setRenaming(false)} centered>
                <Div className="vbox dialog">
                    <Input text={renameDraft} onChanged={setRenameDraft}/>
                    <Div className="dialog__actions">
                        <Button className="dialog__action" text="Cancel" variant="primary" onClicked={() => setRenaming(false)}/>
                        <Button className="dialog__action" text="Save" variant="secondary" onClicked={onRename}/>
                    </Div>
                </Div>
            </Popup>

            <Popup open={confirmingDelete} onClose={() => setConfirmingDelete(false)} centered>
                <Div className="vbox dialog">
                    <Label text={`Delete folder "${folderName}"? Its chats stay, just ungrouped.`}/>
                    <Div className="dialog__actions">
                        <Button className="dialog__action" text="Cancel" variant="primary" onClicked={() => setConfirmingDelete(false)}/>
                        <Button className="dialog__action" text="Continue" variant="secondary" onClicked={onDelete}/>
                    </Div>
                </Div>
            </Popup>

            <Popup open={assigningChat != null} onClose={() => setAssigningChat(null)} centered>
                <Div className="dos-frame sidebar__folder-picker">
                    <span className="dos-frame__title">Assign folder</span>
                    <Div className="dos-frame__body">
                        <FolderPicker
                            selected={assigningChat?.folder_id ?? null}
                            onSelect={async (chosen) => {
                                if (assigningChat) await setChatFolder(assigningChat.id, chosen)
                                setAssigningChat(null)
                            }}
                        />
                    </Div>
                </Div>
            </Popup>

            <Popup open={creatingChat} onClose={() => setCreatingChat(false)} centered>
                <Div className="dos-frame sidebar__folder-picker">
                    <span className="dos-frame__title">New chat</span>
                    <Div className="dos-frame__body">
                        <CreateChatForm
                            createChat={createChat}
                            onCreated={onChatCreated}
                            onCancel={() => setCreatingChat(false)}
                        />
                    </Div>
                </Div>
            </Popup>
        </Div>
    )
};

export default Folder
