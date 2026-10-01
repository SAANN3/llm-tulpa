import {useState} from 'react'

import '../styles/folder-picker.scss'
import {chatName as chatNameApi} from '../api/prompts/chat-name'
import type {ChatOut} from '../api/chats/types'
import {Button, Div, Input, Label} from './primitives'
import {PopupActions} from './popups/base/popup-actions.tsx'

export interface CreateChatFormProps {
    createChat: (name: string) => Promise<ChatOut>
    onCreated: (chat: ChatOut) => void
    onCancel: () => void
}

type CreateMode = 'describe' | 'name'

/** Either type the chat's name directly, or describe what it's for and let the model
 * name it — shared by the folder page's new-chat button. */
export const CreateChatForm = ({createChat, onCreated, onCancel}: CreateChatFormProps) => {
    const [mode, setMode] = useState<CreateMode>('describe')
    const [draft, setDraft] = useState('')
    const [generating, setGenerating] = useState(false)
    const [error, setError] = useState<string | null>(null)

    const onCreate = async () => {
        const text = draft.trim()
        if (!text) return

        setError(null)
        try {
            let name = text
            if (mode === 'describe') {
                setGenerating(true)
                name = (await chatNameApi(text)).response.trim() || text
            }
            onCreated(await createChat(name))
        } catch {
            setError('Could not create the chat.')
        } finally {
            setGenerating(false)
        }
    }

    return (
        <Div className="vbox folder-picker__create">
            <Div className="folder-picker__mode">
                <Button variant={mode === 'describe' ? undefined : 'secondary'} text="Describe it" onClicked={() => setMode('describe')}/>
                <Button variant={mode === 'name' ? undefined : 'secondary'} text="Write name yourself" onClicked={() => setMode('name')}/>
            </Div>
            <Input
                autoFocus
                text={draft}
                onChanged={setDraft}
                placeholder={mode === 'describe' ? "What's this chat about? We'll name it." : 'Chat name'}
            />
            {error ? <Label variant="secondary" className="folder-picker__error" text={error}/> : null}
            <PopupActions
                emphasis="confirm"
                confirmLabel={generating ? 'Thinking…' : 'Create'}
                confirmDisabled={!draft.trim() || generating}
                onConfirm={onCreate}
                onCancel={onCancel}
            />
        </Div>
    )
};
