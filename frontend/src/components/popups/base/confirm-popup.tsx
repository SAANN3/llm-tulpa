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
export const ConfirmPopup = ({open, title, message, confirmLabel = 'Continue', onConfirm, onClose}: ConfirmPopupProps) => {
    const confirm = () => {
        onClose()
        onConfirm()
    }

    return (
        <Popup open={open} onClose={onClose} title={title}
               actions={[{keys: ['Enter'], shown: 'enter', label: confirmLabel.toLowerCase(), run: confirm}]}>
            <Label text={message}/>
            <PopupActions confirmLabel={confirmLabel} onConfirm={confirm} onCancel={onClose}/>
        </Popup>
    )
};
