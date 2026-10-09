import {useState} from 'react'
import axios from 'axios'
import {createUser} from '../../api/users/create'
import {PasswordInput} from '../password-input.tsx'
import {Input, Label} from '../primitives'
import {Popup} from './base/popup.tsx'
import {PopupActions} from './base/popup-actions.tsx'

export interface NewUserPopupProps {
    open: boolean
    onClose: () => void
    /** After the account exists */
    onCreated: () => void
}

/** The form, mounted only while the popup is open, so it starts empty each time */
const NewUserForm = ({onClose, onCreated}: Omit<NewUserPopupProps, 'open'>) => {
    const [username, setUsername] = useState('')
    const [password, setPassword] = useState('')
    const [error, setError] = useState<string | null>(null)
    const [busy, setBusy] = useState(false)
    const canCreate = username.trim().length > 0 && password.length > 0 && !busy

    const create = async () => {
        if (!canCreate) return
        setBusy(true)
        setError(null)
        try {
            await createUser(username.trim(), password)
            onCreated()
            onClose()
        } catch (e) {
            const status = axios.isAxiosError(e) ? e.response?.status : undefined
            setError(status === 409 ? 'A user with that name already exists.' : 'Could not create the user.')
        } finally {
            setBusy(false)
        }
    }

    return (
        <>
            <Input text={username} onChanged={setUsername} placeholder="Username" autoFocus
                   onKeyDown={(e) => e.key === 'Enter' && void create()}/>
            <PasswordInput text={password} onChanged={setPassword} placeholder="Password"
                           onKeyDown={(e) => e.key === 'Enter' && void create()}/>
            <Label variant="secondary" className="field__help"
                   text={error ?? 'They sign in with these. A new account starts with no chats and the default settings.'}/>
            <PopupActions confirmLabel={busy ? 'Creating…' : 'Create'} onConfirm={() => void create()}
                          confirmDisabled={!canCreate} onCancel={onClose} emphasis="confirm"/>
        </>
    )
};

/** Creates an account for someone else; owner-only, like the page it opens from */
export const NewUserPopup = ({open, onClose, onCreated}: NewUserPopupProps) => (
    <Popup open={open} onClose={onClose} title="New user" actions={[{keys: [], shown: 'enter', label: 'create'}]}>
        <NewUserForm onClose={onClose} onCreated={onCreated}/>
    </Popup>
);
