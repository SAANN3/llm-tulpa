import {useEffect, useState} from 'react'
import {browseFolders, type BrowsedFolders} from '../../api/runtime/folder'
import {errorReason} from '../../utils/error-reason.ts'
import {Button, Div, Input, Label} from '../primitives'
import '../../styles/tree.scss'
import {Popup} from './base/popup.tsx'

export interface ModelFolderPopupProps {
    open: boolean
    /** The folder to start from, when there is one */
    current: string | null
    /** Rejects with the backend's reason when the folder is refused */
    onChoose: (path: string) => Promise<void>
    onClose: () => void
}

/** How long the typed path has to stay the same before it is opened */
const TYPED_OPEN_MS = 300

// Its own component so the browsing state starts afresh each time the popup opens
const FolderBrowser = ({current, onChoose, onClose}: Omit<ModelFolderPopupProps, 'open'>) => {
    const [listing, setListing] = useState<BrowsedFolders | null>(null)
    const [typed, setTyped] = useState(current ?? '')
    const [error, setError] = useState<string | null>(null)
    const [busy, setBusy] = useState(false)

    // `quiet`: the path comes from typing, where a half-typed one is normal and says nothing; the field keeps what
    // was typed. Otherwise (a click, Enter) the field shows where the listing is, and a failure says why.
    const go = (path: string | undefined, quiet = false) => {
        if (!quiet) setError(null)
        browseFolders(path).then((result) => {
            setListing(result)
            if (!quiet) setTyped(result.path)
            else setError(null)
        }).catch((e) => {
            if (!quiet) setError(errorReason(e, 'Could not open that folder.'))
        })
    }

    useEffect(() => {
        go(current ?? undefined)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [])

    // A typed path that names a folder opens it, so the list follows the typing without an Open button
    useEffect(() => {
        const path = typed.trim()
        if (!path || path === listing?.path) return
        const timer = setTimeout(() => go(path, true), TYPED_OPEN_MS)
        return () => clearTimeout(timer)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [typed])

    const choose = async () => {
        setBusy(true)
        setError(null)
        try {
            await onChoose(typed.trim())
            onClose()
        } catch (e) {
            setError(errorReason(e, 'Could not use that folder.'))
        } finally {
            setBusy(false)
        }
    }

    return (
        <>
            <Input className="models__folder-path" text={typed} onChanged={setTyped} placeholder="/path/to/models"
                   onKeyDown={(e) => e.key === 'Enter' && go(typed.trim())}/>
            {/* One level, drawn like the other lists: the folder's own sub-folders, with the way up first */}
            <Div className="models__folders vbox tree">
                {listing?.parent != null ? (
                    <Div className="tree__row">
                        <Div className="list-row" onClick={() => go(listing.parent ?? undefined)}>..</Div>
                    </Div>
                ) : null}
                {listing?.folders.map((f) => (
                    <Div key={f.path} className="tree__row">
                        <Div className="list-row" onClick={() => go(f.path)}>{`${f.name}/`}</Div>
                    </Div>
                ))}
            </Div>
            {listing?.folders.length === 0 ? <Label variant="secondary" text="No folders inside." className="models__folders-empty"/> : null}
            <Label variant="secondary" className="field__help"
                   text="Models run from this folder and downloads land in it. Under Docker only folders the container can see are possible, such as /models or something under /home."/>
            {error ? <Label variant="secondary" className="models__error" text={error}/> : null}
            <Div className="popup__actions">
                <Button variant="secondary" text="Cancel" onClicked={onClose}/>
                <Button text={busy ? 'Using…' : 'Use this folder'} disabled={busy || !typed.trim()} onClicked={() => void choose()}/>
            </Div>
        </>
    )
}

/** Pick the folder the model files live in: type a path (it opens as it is typed) or walk the folders */
export const ModelFolderPopup = ({open, onClose, ...rest}: ModelFolderPopupProps) => (
    // Enter is the path field's own key, so the footer only names it
    <Popup open={open} onClose={onClose} title="Model folder" width={640}
           actions={[{keys: ['Enter'], shown: 'enter', label: 'open the typed path'}]}>
        <FolderBrowser {...rest} onClose={onClose}/>
    </Popup>
);
