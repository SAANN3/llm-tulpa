import {CreateChatForm, type CreateChatFormProps} from '../create-chat-form.tsx'
import {Popup} from './base/popup.tsx'

export interface NewChatPopupProps extends Omit<CreateChatFormProps, 'onCancel'> {
    open: boolean
    onClose: () => void
}

/** Name a new chat, directly or by describing it */
export const NewChatPopup = ({open, onClose, createChat, onCreated}: NewChatPopupProps) => (
    <Popup open={open} onClose={onClose} title="New chat">
        <CreateChatForm createChat={createChat} onCreated={onCreated} onCancel={onClose}/>
    </Popup>
);
