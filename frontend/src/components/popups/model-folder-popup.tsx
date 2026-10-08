import {useEffect, useState} from 'react'
import {browseFolders, type BrowsedFolders} from '../../api/runtime/folder'
import {errorReason} from '../../utils/error-reason.ts'
import {Button, Div, Input, Label} from '../primitives'
import {Popup} from './base/popup.tsx'

export interface ModelFolderPopupProps {
    open: boolean
    /** The folder to start from, when there is one */
    current: string | null
    /** Rejects with the backend's reason when the folder is refused */
    onChoose: (path: string) => Promise<void>
    onClose: () => void
}

// Its own component so the browsing state starts afresh each time the popup opens
const FolderBrowser = ({current, onChoose, onClose}: Omit<ModelFolderPopupProps, 'open'>) => {
    const [listing, setListing] = useState<BrowsedFolders | null>(null)
    const [typed, setTyped] = useState(current ?? '')
    const [error, setError] = useState<string | null>(null)
    const [busy, setBusy] = useState(false)

    const go = (path?: string) => {
        setError(null)
        browseFolders(path).then((result) => {
            setListing(result)
            setTyped(result.path)
        }).catch((e) => setError(errorReason(e, 'Could not open that folder.')))
    }

    useEffect(() => {
        go(current ?? undefined)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [])

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
            <Div className="models__toolbar">
                <Input text={typed} onChanged={setTyped} placeholder="/path/to/models"
                       onKeyDown={(e) => e.key === 'Enter' && go(typed.trim())}/>
                <Button variant="secondary" text="Open" onClicked={() => go(typed.trim())}/>
            </Div>
            <Div className="models__folders">
                {listing?.parent != null ? (
                    <Button variant="secondary" text="Up one folder (..)" onClicked={() => go(listing.parent ?? undefined)}/>
                ) : null}
                {listing?.folders.length === 0 ? <Label variant="secondary" text="No folders inside." className="models__folders-empty"/> : null}
                {listing?.folders.map((f) => (
                    <Button key={f.path} variant="secondary" text={f.name} onClicked={() => go(f.path)}/>
                ))}
            </Div>
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

/** Pick the folder the model files live in: type a path or walk the folders */
export const ModelFolderPopup = ({open, onClose, ...rest}: ModelFolderPopupProps) => (
    // Enter is the path field's own key, so the footer only names it
    <Popup open={open} onClose={onClose} title="Model folder" width={520}
           actions={[{keys: ['Enter'], shown: 'enter', label: 'open the typed path'}]}>
        <FolderBrowser {...rest} onClose={onClose}/>
    </Popup>
);
