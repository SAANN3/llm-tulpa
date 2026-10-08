import {useEffect, useState} from 'react'
import axios from 'axios'

import '../styles/users.scss'
import {Frame} from '../components/frame.tsx'
import {ConfirmPopup} from '../components/popups/base/confirm-popup.tsx'
import type {User} from '../api/auth/types'
import {createUser} from '../api/users/create'
import {deleteUser} from '../api/users/delete'
import {listUsers} from '../api/users/list'
import {Button, Div, Input, Label} from '../components/primitives'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useAuth} from '../context/use-auth.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {useGoBack} from '../hooks/use-go-back.ts'
import {PasswordInput} from '../components/password-input.tsx'

const Users = () => {
    useDocumentTitle('Users')
    const goBack = useGoBack()
    const {user: currentUser} = useAuth()
    const [users, setUsers] = useState<User[]>([])
    const [username, setUsername] = useState('')
    const [password, setPassword] = useState('')
    const [error, setError] = useState<string | null>(null)
    const [busy, setBusy] = useState(false)
    const [confirmingId, setConfirmingId] = useState<number | null>(null)

    const refresh = () => listUsers().then(setUsers).catch(() => setError('Could not load users.'))

    useEffect(() => {
        refresh()
    }, [])

    const canCreate = username.trim().length > 0 && password.length > 0 && !busy

    const onCreate = async () => {
        if (!canCreate) return
        setBusy(true)
        setError(null)
        try {
            await createUser(username.trim(), password)
            setUsername('')
            setPassword('')
            await refresh()
        } catch (e) {
            const status = axios.isAxiosError(e) ? e.response?.status : undefined
            setError(status === 409 ? 'A user with that name already exists.' : 'Could not create the user.')
        } finally {
            setBusy(false)
        }
    }

    const confirming = users.find((u) => u.id === confirmingId) ?? null

    const onDelete = async (id: number) => {
        setError(null)
        setConfirmingId(null)
        try {
            await deleteUser(id)
            await refresh()
        } catch {
            setError('Could not delete the user.')
        }
    }

    return (
        <Div className="page center vbox users">
            <TypewriterLabel className="users__title" text="[ Users ]" charIntervalMs={30}/>
            <Frame className="users__panel" bodyClassName="users__body" title="Users"
                   actions={[]} onEscape={goBack} escapeLabel="back">
                <Div className="users__list">
                    {users.map((u) => (
                        <Div key={u.id} className="users__row">
                            <Label className="users__name" text={u.username}/>
                            <Label variant="secondary" className="users__role" text={u.role}/>
                            {u.id === currentUser?.id ? (
                                <Label variant="secondary" className="users__you" text="(you)"/>
                            ) : (
                                <Button variant="secondary" text="Delete" onClicked={() => setConfirmingId(u.id)}/>
                            )}
                        </Div>
                    ))}
                </Div>

                <Div className="field">
                    <Label className="field__label" text="New user"/>
                    <Input text={username} onChanged={setUsername} placeholder="Username"/>
                    <PasswordInput text={password} onChanged={setPassword} placeholder="Password"/>
                </Div>

                {error ? <Label variant="secondary" className="users__error" text={error}/> : null}

                <Div className="users__actions">
                    <Button variant="secondary" text="Back" onClicked={goBack}/>
                    <Button text={busy ? 'Creating…' : 'Create user'} onClicked={onCreate} disabled={!canCreate}/>
                </Div>
            </Frame>
            <ConfirmPopup open={confirming != null} title="Delete user" confirmLabel="Delete"
                          message={`Delete ${confirming?.username ?? ''} and their chats?`}
                          onConfirm={() => {
                              if (confirming) void onDelete(confirming.id)
                          }}
                          onClose={() => setConfirmingId(null)}/>
        </Div>
    )
};

export default Users
