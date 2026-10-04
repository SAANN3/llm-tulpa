import {useState} from 'react'
import {errorReason} from '../../utils/error-reason.ts'
import {Button, Div, Label} from '../primitives'
import {Popup} from './base/popup.tsx'
import {PopupActions} from './base/popup-actions.tsx'

export interface RemoveModelPopupProps {
    open: boolean
    /** What the model is called in the list */
    name: string
    /** Its file is already gone from the model folder, so there is nothing to delete */
    fileMissing: boolean
    /** Rejects with the backend's reason when the model can't be removed */
    onRemove: (deleteFile: boolean) => Promise<void>
    onClose: () => void
}

// Its own component so the choice starts afresh each time the popup opens
const RemoveForm = ({name, fileMissing, onRemove, onClose}: Omit<RemoveModelPopupProps, 'open'>) => {
    // What the owner picked, until they confirm it: the removal is asked twice, once for how and once for sure
    const [choice, setChoice] = useState<'keep' | 'delete' | null>(null)
    const [busy, setBusy] = useState(false)
    const [error, setError] = useState<string | null>(null)

    const confirm = async () => {
        if (!choice) return
        setBusy(true)
        setError(null)
        try {
            await onRemove(choice === 'delete')
            onClose()
        } catch (e) {
            setError(errorReason(e, 'Could not remove the model.'))
        } finally {
            setBusy(false)
        }
    }

    if (!choice) {
        return (
            <>
                <Label text={`Remove "${name}"?`}/>
                <Label variant="secondary" className="field__help"
                       text="Its launch profiles go with it. Chats using it move to your default model, and its sampling presets stay as presets for any model."/>
                {fileMissing ? (
                    <Label variant="secondary" className="models__error" text="Its file is no longer in the model folder, so only the entry can be removed."/>
                ) : null}
                <Div className="popup__actions popup__actions--stacked">
                    <Button variant="secondary" text="Remove from the list, keep the file" onClicked={() => setChoice('keep')}/>
                    {fileMissing ? null : <Button variant="secondary" text="Remove and delete the file" onClicked={() => setChoice('delete')}/>}
                    <Button variant="primary" text="Cancel" onClicked={onClose}/>
                </Div>
            </>
        )
    }

    return (
        <>
            <Label text={choice === 'delete'
                ? `Are you sure you want to remove "${name}" and delete its file from the disk? The file cannot be brought back.`
                : `Are you sure you want to remove "${name}" from the list? Its file stays in the model folder.`}/>
            {error ? <Label variant="secondary" className="models__error" text={error}/> : null}
            <PopupActions confirmLabel={busy ? 'Removing…' : choice === 'delete' ? 'Delete' : 'Remove'} confirmDisabled={busy}
                          onConfirm={() => void confirm()} onCancel={() => setChoice(null)} cancelLabel="Back"/>
        </>
    )
}

/** Removes a model from the list, keeping its file or deleting it, with a second question before either */
export const RemoveModelPopup = ({open, onClose, ...rest}: RemoveModelPopupProps) => (
    <Popup open={open} onClose={onClose} title="Remove model" width={460}>
        <RemoveForm {...rest} onClose={onClose}/>
    </Popup>
);
