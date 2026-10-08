import {Reload} from 'pixelarticons/react'
import {Button, Div, Label} from '../primitives'
import {Popup} from './base/popup.tsx'

export interface SetupUpdatePopupProps {
    open: boolean
    onSetup: () => void
    onLater: () => void
}

/** Shown to the owner once after an update that changed what the setup wizard configures */
export const SetupUpdatePopup = ({open, onSetup, onLater}: SetupUpdatePopupProps) => (
    <Popup open={open} onClose={onLater} title="We updated the app"
           actions={[{keys: ['Enter'], shown: 'enter', label: 'set up now', run: onSetup}]}>
        <Div className="center">
            <Reload width={48} height={48}/>
        </Div>
        <Label text="We updated our codebase, and some changes may need a little setup. Your existing chats are safe."/>
        <Label variant="secondary" text="The wizard keeps your current answers and only asks about what is new. It can also move your existing chats to the new model."/>
        <Div className="popup__actions">
            <Button variant="secondary" text="Later" onClicked={onLater}/>
            <Button text="Set up now" onClicked={onSetup}/>
        </Div>
    </Popup>
);
