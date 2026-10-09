import {useEffect, useRef, useState} from 'react'
import {ArrowLeft, ChevronDown, Close, ExternalLink, Folder, InfoBox, Pencil, Search, Share, Tools, Trash} from 'pixelarticons/react'
import {useNavigate} from 'react-router-dom'

import '../styles/chat-header.scss'
import {deleteChat} from '../api/chats/delete'
import {renameChat} from '../api/chats/rename'
import {setChatFolder} from '../api/chats/set-folder'
import {setChatTools} from '../api/chats/set-tools'
import type {MessageSearchOut} from '../api/chats/types'
import {getFolders} from '../api/folders/get'
import {formatTokenCount} from '../utils/format.ts'
import {AssignFolderPopup} from './popups/assign-folder-popup.tsx'
import {ConfirmPopup} from './popups/base/confirm-popup.tsx'
import {ContextMenu, type ContextMenuItem} from './popups/base/context-menu.tsx'
import {ExportChatPopup} from './popups/export-chat-popup.tsx'
import {InputPopup} from './popups/base/input-popup.tsx'
import {SearchMessagesPopup} from './popups/search-messages-popup.tsx'
import {Button, Div, Label} from './primitives'

export interface ChatHeaderProps {
    chatId: number
    name: string | null
    /** How much context the chat is using (Ollama's last measured prompt size), null while unknown */
    contextUsed: number | null
    /** The context window the agent runs under — the gauge's max */
    contextMax: number | null
    /** The folder this chat is grouped under, or null if ungrouped */
    folderId: number | null
    onFolderChanged: (folderId: number | null) => void
    /** The chat this one is a sub-agent's chat of, or null for an ordinary chat */
    parentChatId: number | null
    /** Scroll the chat to the message of a clicked search hit */
    onSelectSearchResult: (hit: MessageSearchOut, query: string) => void
    /** Whether a search hit is currently highlighted in the timeline — shows the clear control */
    hasActiveHighlight: boolean
    /** Turns off the current search highlight */
    onClearHighlight: () => void
    /** Whether the model is sent its tools in this chat */
    toolsEnabled: boolean
    onToolsChanged: (enabled: boolean) => void
    /** The chat has a run going on: its tools can't be switched until it ends */
    runActive: boolean
}

/** The bar above a chat's messages: its name, which opens the chat's menu (search, rename, export, tools, folder,
 * delete), and the context gauge */
export const ChatHeader = ({
    chatId,
    name,
    contextUsed,
    contextMax,
    folderId,
    onFolderChanged,
    parentChatId,
    onSelectSearchResult,
    hasActiveHighlight,
    onClearHighlight,
    toolsEnabled,
    onToolsChanged,
    runActive,
}: ChatHeaderProps) => {
    const navigate = useNavigate()
    const [folderOpen, setFolderOpen] = useState(false)
    const [searchOpen, setSearchOpen] = useState(false)
    const [exportOpen, setExportOpen] = useState(false)
    const [renameOpen, setRenameOpen] = useState(false)
    const [folderName, setFolderName] = useState<string | null>(null)
    const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null)
    const [confirmingDelete, setConfirmingDelete] = useState(false)
    const titleRef = useRef<HTMLDivElement>(null)

    useEffect(() => {
        if (folderId == null) {
            setFolderName(null)
            return
        }
        let cancelled = false
        getFolders({id: folderId}).then((result) => {
            if (!cancelled && !('folders' in result)) setFolderName(result.name)
        })
        return () => {
            cancelled = true
        }
    }, [folderId])

    const onSelectFolder = async (chosen: number | null) => {
        await setChatFolder(chatId, chosen)
        onFolderChanged(chosen)
        setFolderOpen(false)
    }

    const toggleTools = async () => {
        await setChatTools(chatId, !toolsEnabled)
        onToolsChanged(!toolsEnabled)
    }

    // The chat list follows by itself (`chat_deleted`); this page has nothing left to show
    const doDelete = async () => {
        await deleteChat(chatId)
        navigate('/')
    }

    const doRename = async (newName: string) => {
        await renameChat(chatId, newName)
    }

    const openMenu = () => {
        const rect = titleRef.current?.getBoundingClientRect()
        if (rect) setMenuAt({x: rect.left, y: rect.bottom + 6})
    }

    const menu: ContextMenuItem[] = [
        {label: 'Search in chat', icon: <Search width={16} height={16}/>, onSelect: () => setSearchOpen(true)},
        {label: 'Chat info', icon: <InfoBox width={16} height={16}/>, onSelect: () => navigate(`/chat/${chatId}/info/context`)},
        {label: 'Rename', icon: <Pencil width={16} height={16}/>, onSelect: () => setRenameOpen(true)},
        {label: 'Export', icon: <Share width={16} height={16}/>, onSelect: () => setExportOpen(true)},
        // A sub-agent's chat has the tools its parent gave it
        ...(parentChatId == null ? [{
            label: runActive ? 'Tools (after this run)' : 'Tools', icon: <Tools width={16} height={16}/>, toggled: toolsEnabled,
            // Switched only between runs: a run keeps the tools it started with
            disabled: runActive, onSelect: () => void toggleTools(),
        }] : []),
        {label: folderId != null ? `Folder: ${folderName ?? '…'}` : 'Move to folder', icon: <Folder width={16} height={16}/>, onSelect: () => setFolderOpen(true)},
        ...(folderId != null ? [{label: 'Open the folder', icon: <ExternalLink width={16} height={16}/>, onSelect: () => navigate(`/folders/${folderId}`)}] : []),
        {label: 'Delete chat', icon: <Trash width={16} height={16}/>, danger: true, onSelect: () => setConfirmingDelete(true)},
    ]

    return (
        <Div className="chat-header">
            {parentChatId != null ? (
                <Button variant="secondary" className="chat-header__parent"
                        onClicked={() => navigate(`/chat?id=${parentChatId}`)}>
                    <ArrowLeft width={16} height={16}/>
                    <span>parent chat</span>
                </Button>
            ) : null}
            {/* The chat's name is the menu: everything about the chat but its context is one click behind it */}
            <Div ref={titleRef} className="chat-header__title-group" onMouseDown={(e) => menuAt && e.stopPropagation()}>
                <Button variant="secondary" className="chat-header__title" title="Chat menu"
                        onClicked={() => (menuAt ? setMenuAt(null) : openMenu())}>
                    <span className="chat-header__name">{name ?? 'Chat'}</span>
                    <ChevronDown width={14} height={14}/>
                </Button>
            </Div>
            {contextMax != null ? (
                <Div className="chat-header__context">
                    <Label className="chat-header__context-label"
                           text={`context ${contextUsed != null ? formatTokenCount(contextUsed) : '—'} / ${formatTokenCount(contextMax)}`}/>
                    <Div className="chat-header__context-bar">
                        <Div className="chat-header__context-fill"
                             style={{width: `${contextUsed != null ? Math.min(100, (contextUsed / contextMax) * 100) : 0}%`}}/>
                    </Div>
                </Div>
            ) : null}
            {hasActiveHighlight ? (
                <Button
                    className="chat-header__search-clear"
                    variant="secondary"
                    title="Clear the search highlight"
                    onClicked={onClearHighlight}
                >
                    <Close width={20} height={20}/>
                </Button>
            ) : null}
            <ContextMenu position={menuAt} onClose={() => setMenuAt(null)} items={menu}/>
            <ConfirmPopup open={confirmingDelete} title="Delete chat" confirmLabel="Delete"
                          message={`Are you sure that you want to delete "${name ?? 'this chat'}"`}
                          onConfirm={() => void doDelete()} onClose={() => setConfirmingDelete(false)}/>
            <AssignFolderPopup
                open={folderOpen}
                selected={folderId}
                onSelect={onSelectFolder}
                onClose={() => setFolderOpen(false)}
            />
            <InputPopup
                open={renameOpen}
                title="Rename chat"
                value={name ?? 'Chat'}
                onSubmit={doRename}
                onClose={() => setRenameOpen(false)}
            />
            <ExportChatPopup open={exportOpen} chatId={chatId} chatName={name} onClose={() => setExportOpen(false)}/>
            <SearchMessagesPopup
                open={searchOpen}
                chatId={chatId}
                onSelect={onSelectSearchResult}
                onClose={() => setSearchOpen(false)}
            />
        </Div>
    )
};
