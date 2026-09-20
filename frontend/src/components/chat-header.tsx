import {useState} from 'react'

import '../styles/chat-header.scss'
import {setChatModel} from '../api/chats/set-model'
import {Button, Div, Label} from './primitives'
import {ModelPicker} from './model-picker.tsx'
import {Popup} from './popup.tsx'

export interface ChatHeaderProps {
    chatId: number
    name: string | null
    /** The model this chat is bound to, or null while it's still loading */
    model: string | null
    provider: string
    onModelChanged: (model: string) => void
}

/** The bar above a chat's messages: its name plus the model it's bound to, with a switcher popup */
export const ChatHeader = ({chatId, name, model, provider, onModelChanged}: ChatHeaderProps) => {
    const [open, setOpen] = useState(false)

    const onSelect = async (chosen: string) => {
        await setChatModel(chatId, chosen, provider)
        onModelChanged(chosen)
        setOpen(false)
    }

    return (
        <Div className="chat-header">
            <Label className="chat-header__name" text={name ?? 'Chat'}/>
            <Button variant="secondary" className="chat-header__model" onClicked={() => setOpen(true)}>
                <span className="chat-header__model-label">model:</span>
                <span className="chat-header__model-name">{model ?? '…'}</span>
            </Button>
            <Popup open={open} onClose={() => setOpen(false)} centered>
                <Div className="dos-frame chat-header__picker">
                    <span className="dos-frame__title">Choose model</span>
                    <Div className="dos-frame__body">
                        <ModelPicker selected={model} onSelect={onSelect}/>
                    </Div>
                </Div>
            </Popup>
        </Div>
    )
};
