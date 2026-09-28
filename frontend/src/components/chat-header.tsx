import {useEffect, useState} from 'react'
import {Close, ExternalLink, Folder, Search} from 'pixelarticons/react'
import {useNavigate} from 'react-router-dom'

import '../styles/chat-header.scss'
import {setChatFolder} from '../api/chats/set-folder'
import {setChatModel} from '../api/chats/set-model'
import type {MessageSearchOut} from '../api/chats/types'
import {getFolders} from '../api/folders/get'
import {formatTokenCount} from '../utils/format.ts'
import {ChatSearch} from './chat-search.tsx'
import {FolderPicker} from './folder-picker.tsx'
import {ModelPicker} from './model-picker.tsx'
import {Popup} from './popup.tsx'
import {Button, Div, Label} from './primitives'

export interface ChatHeaderProps {
    chatId: number
    name: string | null
    /** The model this chat is bound to, or null while it's still loading */
    model: string | null
    provider: string
    onModelChanged: (model: string) => void
    /** How much context the chat is using (Ollama's last measured prompt size), null while unknown */
    contextUsed: number | null
    /** The context window the agent runs under — the gauge's max */
    contextMax: number | null
    /** The folder this chat is grouped under, or null if ungrouped */
    folderId: number | null
    onFolderChanged: (folderId: number | null) => void
    /** Scroll the chat to the message of a clicked search hit */
    onSelectSearchResult: (hit: MessageSearchOut, query: string) => void
    /** Whether a search hit is currently highlighted in the timeline — shows the clear control */
    hasActiveHighlight: boolean
    /** Turns off the current search highlight */
    onClearHighlight: () => void
}

/** The bar above a chat's messages: its name, the model it's bound to (with a switcher popup),
 * and in-chat message search */
export const ChatHeader = ({
    chatId,
    name,
    model,
    provider,
    onModelChanged,
    contextUsed,
    contextMax,
    folderId,
    onFolderChanged,
    onSelectSearchResult,
    hasActiveHighlight,
    onClearHighlight,
}: ChatHeaderProps) => {
    const navigate = useNavigate()
    const [open, setOpen] = useState(false)
    const [folderOpen, setFolderOpen] = useState(false)
    const [searchOpen, setSearchOpen] = useState(false)
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

    const onSelect = async (chosen: string) => {
        await setChatModel(chatId, chosen, provider)
        onModelChanged(chosen)
        setOpen(false)
    }

    const onSelectFolder = async (chosen: number | null) => {
        await setChatFolder(chatId, chosen)
        onFolderChanged(chosen)
        setFolderOpen(false)
    }

    return (
        <Div className="chat-header">
            <Label className="chat-header__name" text={name ?? 'Chat'}/>
            <Button variant="secondary" className="chat-header__model" onClicked={() => setOpen(true)}>
                <span className="chat-header__model-label">model:</span>
                <span className="chat-header__model-name">{model ?? '…'}</span>
            </Button>
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
            <Popup open={open} onClose={() => setOpen(false)} centered>
                <Div className="dos-frame chat-header__picker">
                    <span className="dos-frame__title">Choose model</span>
                    <Div className="dos-frame__body">
                        <ModelPicker selected={model} onSelect={onSelect}/>
                    </Div>
                </Div>
            </Popup>
            <Popup open={folderOpen} onClose={() => setFolderOpen(false)} centered>
                <Div className="dos-frame chat-header__picker">
                    <span className="dos-frame__title">Assign folder</span>
                    <Div className="dos-frame__body">
                        <FolderPicker selected={folderId} onSelect={onSelectFolder}/>
                    </Div>
                </Div>
            </Popup>
            <Popup open={searchOpen} onClose={() => setSearchOpen(false)} centered>
                <Div className="dos-frame chat-header__search-box">
                    <span className="dos-frame__title">Search messages</span>
                    <Div className="dos-frame__body">
                        <ChatSearch
                            chatId={chatId}
                            onSelect={(hit, query) => {
                                setSearchOpen(false)
                                onSelectSearchResult(hit, query)
                            }}
                            onClose={() => setSearchOpen(false)}/>
                    </Div>
                </Div>
            </Popup>
        </Div>
    )
};
