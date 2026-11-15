import {Button, Div, Label} from '../primitives'
import {Popup} from './base/popup'

interface UnsavedChangesPopupProps {
    open: boolean
    saving: boolean
    saveDisabled: boolean
    onClose: () => void
    onDiscard: () => void
    onSave: () => void
}

export const UnsavedChangesPopup = ({open, saving, saveDisabled, onClose, onDiscard, onSave}: UnsavedChangesPopupProps) => (
    <Popup open={open} title="Unsaved changes" onClose={() => { if (!saving) onClose() }}
           actions={[{keys: ['Enter'], shown: 'enter', label: 'save', run: () => { if (!saving && !saveDisabled) onSave() }}]}>
        <Label text="You have unsaved changes. Save them before continuing?"/>
        <Div className="popup__actions">
            <Button text="Cancel" variant="secondary" disabled={saving} onClicked={onClose}/>
            <Button text="Discard" variant="secondary" disabled={saving} onClicked={onDiscard}/>
            <Button text={saving ? 'Saving…' : 'Save'} disabled={saving || saveDisabled} onClicked={onSave}/>
        </Div>
    </Popup>
)
