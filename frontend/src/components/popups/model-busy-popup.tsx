import {Button, Div, Label} from '../primitives'
import {Popup} from './base/popup.tsx'

export interface ModelBusyPopupProps {
    /** The backend's explanation (who has the model server), or null while nothing is wrong */
    reason: string | null
    onClose: () => void
}

/** Tells the user the model server is loaded with a different model that another user is in the
 * middle of using: switching now would throw away what that turn has cached, so they wait */
export const ModelBusyPopup = ({reason, onClose}: ModelBusyPopupProps) => (
    <Popup open={reason != null} onClose={onClose} title="Model in use">
        <Label text={reason ?? ''}/>
        <Label variant="secondary" text="The model did not answer. Try again in a moment."/>
        <Div className="popup__actions">
            <Button text="OK" onClicked={onClose}/>
        </Div>
    </Popup>
);
