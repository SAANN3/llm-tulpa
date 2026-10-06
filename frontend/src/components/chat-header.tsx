import {useEffect, useState} from 'react'
import {ArrowLeft, Close, ExternalLink, Folder, Pencil, Search, Share} from 'pixelarticons/react'
import {useNavigate} from 'react-router-dom'

import '../styles/chat-header.scss'
import {renameChat} from '../api/chats/rename'
import {setChatFolder} from '../api/chats/set-folder'
import {setChatModel} from '../api/chats/set-model'
import {setChatProfile} from '../api/chats/set-profile'
import {setChatTools} from '../api/chats/set-tools'
import type {MessageSearchOut} from '../api/chats/types'
import {getFolders} from '../api/folders/get'
import type {LaunchProfile} from '../api/profiles/types'
import {profileLabel, useProfileCatalog} from '../hooks/use-profile-catalog.ts'
import {formatTokenCount} from '../utils/format.ts'
import {AssignFolderPopup} from './popups/assign-folder-popup.tsx'
import {ChooseModelPopup} from './popups/choose-model-popup.tsx'
import {ExportChatPopup} from './popups/export-chat-popup.tsx'
import {InputPopup} from './popups/base/input-popup.tsx'
import {SearchMessagesPopup} from './popups/search-messages-popup.tsx'
import {Button, Div, Label} from './primitives'

export interface ChatHeaderProps {
    chatId: number
    name: string | null
    /** The model this chat is bound to, or null while it's still loading */
    model: string | null
    provider: string
    /** Called with the new model, and with the launch profile it runs under (null for an Ollama model) */
    onModelChanged: (model: string, provider: string, profileId: number | null) => void
    /** The launch profile the chat runs under, or null for a model with none */
    launchProfileId: number | null
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

/** The bar above a chat's messages: its name, the model it's bound to (with a switcher popup),
 * and in-chat message search */
export const ChatHeader = ({
    chatId,
    name,
    model,
    provider,
    onModelChanged,
    launchProfileId,
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
    const [open, setOpen] = useState(false)
    const [folderOpen, setFolderOpen] = useState(false)
    const [searchOpen, setSearchOpen] = useState(false)
    const [exportOpen, setExportOpen] = useState(false)
    const [renameOpen, setRenameOpen] = useState(false)
    const [folderName, setFolderName] = useState<string | null>(null)

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

    const {models, profiles} = useProfileCatalog()

    // An Ollama model: the backend moves the chat off any launch profile
    const onSelect = async (chosen: string) => {
        await setChatModel(chatId, chosen, 'ollama')
        onModelChanged(chosen, 'ollama', null)
        setOpen(false)
    }

    const onSelectProfile = async (profile: LaunchProfile) => {
        await setChatModel(chatId, profile.model, profile.provider)
        await setChatProfile(chatId, profile.id)
        onModelChanged(profile.model, profile.provider, profile.id)
        setOpen(false)
    }

    const onSelectFolder = async (chosen: number | null) => {
        await setChatFolder(chatId, chosen)
        onFolderChanged(chosen)
        setFolderOpen(false)
    }

    const toggleTools = async () => {
        await setChatTools(chatId, !toolsEnabled)
        onToolsChanged(!toolsEnabled)
    }

    const doRename = async (newName: string) => {
        await renameChat(chatId, newName)
    }

    return (
        <Div className="chat-header">
            {parentChatId != null ? (
                <Button variant="secondary" className="chat-header__parent"
                        onClicked={() => navigate(`/chat?id=${parentChatId}`)}>
                    <ArrowLeft width={16} height={16}/>
                    <span>parent chat</span>
                </Button>
            ) : null}
            <Div className="chat-header__title-group">
                <Label className="chat-header__name" text={name ?? 'Chat'}/>
                <Button variant="secondary" className="chat-header__rename" onClicked={() => setRenameOpen(true)}>
                    <Pencil width={16} height={16}/>
                </Button>
            </Div>
            <Button variant="secondary" className="chat-header__model" onClicked={() => setOpen(true)}>
                <span className="chat-header__model-label">model:</span>
                <span className="chat-header__model-name">{profileLabel(models, profiles, launchProfileId) ?? model ?? '…'}</span>
            </Button>
            {parentChatId == null ? (
                <Button variant="secondary" className="chat-header__tools" disabled={runActive}
                        title={toolsEnabled ? 'The model is sent its tools in this chat. Click to turn them off.' : 'The model has no tools in this chat. Click to turn them on.'}
                        onClicked={() => void toggleTools()}>
                    <span className="chat-header__model-label">tools:</span>
                    <span className="chat-header__model-name">{toolsEnabled ? 'on' : 'off'}</span>
                </Button>
            ) : null}
            <Div className="chat-header__folder-group">
                <Button variant="secondary" className="chat-header__folder" onClicked={() => setFolderOpen(true)}>
                    <Folder width={16} height={16}/>
                    {folderId != null ? <span className="chat-header__folder-name">{folderName ?? '…'}</span> : null}
                </Button>
                <Button
                    className="chat-header__folder-jump"
                    disabled={folderId == null}
                    onClicked={() => folderId != null && navigate(`/folders/${folderId}`)}
                >
                    <ExternalLink width={16} height={16}/>
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
            <Button className="chat-header__search" title="Export chat"
                    onClicked={() => setExportOpen(true)}>
                <Share width={20} height={20}/>
            </Button>
            <Button className="chat-header__search"
                    onClicked={() => setSearchOpen(true)}>
                <Search width={20} height={20}/>
            </Button>
            {hasActiveHighlight ? (
                <Button
                    className="chat-header__search-clear"
                    variant="secondary"
                    onClicked={onClearHighlight}
                >
                    <Close width={20} height={20}/>
                </Button>
            ) : null}
            <ChooseModelPopup open={open} provider={provider} selected={provider === 'ollama' ? model : null} selectedProfileId={launchProfileId}
                              onSelect={onSelect} onSelectProfile={onSelectProfile} onClose={() => setOpen(false)}/>
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
