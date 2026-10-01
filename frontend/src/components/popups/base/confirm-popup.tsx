import {Label} from '../../primitives'
import {Popup} from './popup.tsx'
import {PopupActions} from './popup-actions.tsx'

export interface ConfirmPopupProps {
    open: boolean
    title: string
    message: string
    confirmLabel?: string
    /** Runs after the popup has closed itself */
    onConfirm: () => void
    onClose: () => void
}

/** Asks the user to confirm something before it happens */
export const ConfirmPopup = ({open, title, message, confirmLabel = 'Continue', onConfirm, onClose}: ConfirmPopupProps) => (
    <Popup open={open} onClose={onClose} title={title}>
        <Label text={message}/>
        <PopupActions
            confirmLabel={confirmLabel}
            onConfirm={() => {
                onClose()
                onConfirm()
            }}
            onCancel={onClose}
        />
    </Popup>
);
