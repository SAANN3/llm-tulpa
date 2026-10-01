import type {MessageSearchOut} from '../../api/chats/types'
import {ChatSearch} from '../chat-search.tsx'
import {Popup} from './base/popup.tsx'

export interface SearchMessagesPopupProps {
    open: boolean
    chatId: number
    /** Runs after the popup has closed itself, with the clicked hit and the query that found it */
    onSelect: (hit: MessageSearchOut, query: string) => void
    onClose: () => void
}

/** Search one chat's messages */
export const SearchMessagesPopup = ({open, chatId, onSelect, onClose}: SearchMessagesPopupProps) => (
    <Popup open={open} onClose={onClose} title="Search messages" width={540}>
        <ChatSearch
            chatId={chatId}
            onSelect={(hit, query) => {
                onClose()
                onSelect(hit, query)
            }}
            onClose={onClose}
        />
    </Popup>
);
