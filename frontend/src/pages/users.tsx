import {useEffect, useState} from 'react'
import {Plus, Trash} from 'pixelarticons/react'

import '../styles/users.scss'
import {Frame} from '../components/frame.tsx'
import {ConfirmPopup} from '../components/popups/base/confirm-popup.tsx'
import {NewUserPopup} from '../components/popups/new-user-popup.tsx'
import type {User} from '../api/auth/types'
import {deleteUser} from '../api/users/delete'
import {listUsers} from '../api/users/list'
import {Button, Div, Label} from '../components/primitives'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useAuth} from '../context/use-auth.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {useGoBack} from '../hooks/use-go-back.ts'

/** The accounts on this server: each as one row, a new one made in a popup. Owner-only. */
const Users = () => {
    useDocumentTitle('Users')
    const goBack = useGoBack()
    const {user: currentUser} = useAuth()
    const [users, setUsers] = useState<User[]>([])
    const [error, setError] = useState<string | null>(null)
    const [creating, setCreating] = useState(false)
    const [confirmingId, setConfirmingId] = useState<number | null>(null)

    const refresh = () => listUsers().then(setUsers).catch(() => setError('Could not load users.'))

    useEffect(() => {
        refresh()
    }, [])

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
                   actions={[{keys: ['n'], label: 'new user', run: () => setCreating(true)}]}
                   onEscape={goBack} escapeLabel="back">
                <Div className="users__head">
                    <Label variant="secondary" className="users__count" text={`${users.length} account${users.length === 1 ? '' : 's'}`}/>
                    <Button variant="secondary" className="users__new" onClicked={() => setCreating(true)}>
                        <Plus width={16} height={16}/>
                        <span>New user</span>
                    </Button>
                </Div>
                <Div className="users__list">
                    {users.map((u) => (
                        <Div key={u.id} className="users__row">
                            <Label className="users__name" text={u.username}/>
                            {u.id === currentUser?.id ? <Label variant="secondary" className="users__you" text="(you)"/> : null}
                            <Label variant="secondary" className="users__role" text={u.role}/>
                            {u.id === currentUser?.id ? (
                                <Div className="users__action-space"/>
                            ) : (
                                <Button variant="secondary" className="users__delete" title={`Delete ${u.username}`}
                                        onClicked={() => setConfirmingId(u.id)}>
                                    <Trash width={16} height={16}/>
                                </Button>
                            )}
                        </Div>
                    ))}
                </Div>

                {error ? <Label variant="secondary" className="users__error" text={error}/> : null}

                <Button variant="secondary" text="Back" onClicked={goBack}/>
            </Frame>
            <NewUserPopup open={creating} onClose={() => setCreating(false)} onCreated={() => void refresh()}/>
            <ConfirmPopup open={confirming != null} title="Delete user" confirmLabel="Delete"
                          message={`Delete ${confirming?.username ?? ''}? Their chats, files and settings go with the account. This can't be undone.`}
                          onConfirm={() => {
                              if (confirming) void onDelete(confirming.id)
                          }}
                          onClose={() => setConfirmingId(null)}/>
        </Div>
    )
};

export default Users
