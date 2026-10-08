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
import {LazyList} from '../components/lazy-list.tsx'
import {AssignFolderPopup} from '../components/popups/assign-folder-popup.tsx'
import {ConfirmPopup} from '../components/popups/base/confirm-popup.tsx'
import {NewChatPopup} from '../components/popups/new-chat-popup.tsx'
import {InputPopup} from '../components/popups/base/input-popup.tsx'
import {Button, Div, Label} from '../components/primitives'
import {Sidebar} from '../components/sidebar.tsx'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useChats} from '../hooks/use-chats.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {useGoBack} from '../hooks/use-go-back.ts'

/** One folder's own page: its chats, newest-active first, plus rename/delete for the
 * folder itself — the destination of a folder row on `/folders` and of a chat header's
 * "jump to folder" button. */
const Folder = () => {
    const {id} = useParams<{ id: string }>()
    const folderId = Number(id)
    const navigate = useNavigate()
    const goBack = useGoBack('/folders')
    const [folderName, setFolderName] = useState<string | null>(null)
    const [renaming, setRenaming] = useState(false)
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

    const onRename = async (name: string) => {
        await renameFolder(folderId, name)
        setFolderName(name)
    }

    const onDelete = async () => {
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
                    <Button variant="secondary" className="folders__back" onClicked={goBack}>
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
                        onClicked={() => setRenaming(true)}
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

            <InputPopup
                open={renaming}
                title="Rename folder"
                value={folderName ?? ''}
                onSubmit={onRename}
                onClose={() => setRenaming(false)}
            />

            <ConfirmPopup
                open={confirmingDelete}
                title="Delete folder"
                message={`Delete folder "${folderName}"? Its chats stay, just ungrouped.`}
                onConfirm={onDelete}
                onClose={() => setConfirmingDelete(false)}
            />

            <AssignFolderPopup
                open={assigningChat != null}
                selected={assigningChat?.folder_id ?? null}
                onSelect={async (chosen) => {
                    if (assigningChat) await setChatFolder(assigningChat.id, chosen)
                    setAssigningChat(null)
                }}
                onClose={() => setAssigningChat(null)}
            />

            <NewChatPopup
                open={creatingChat}
                createChat={createChat}
                onCreated={onChatCreated}
                onClose={() => setCreatingChat(false)}
            />
        </Div>
    )
};

export default Folder
