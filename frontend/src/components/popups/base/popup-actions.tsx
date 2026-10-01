import {Button, Div} from '../../primitives'

export interface PopupActionsProps {
    confirmLabel: string
    onConfirm: () => void
    confirmDisabled?: boolean
    cancelLabel?: string
    onCancel: () => void
    /** Which button carries the highlighted variant: "cancel" for a confirm or rename (the
     * safe choice stands out), "confirm" for a create form (the action to take stands out) */
    emphasis?: 'cancel' | 'confirm'
}

/** The Cancel / confirm row at the bottom of a popup, the two buttons sharing its width */
export const PopupActions = ({
    confirmLabel,
    onConfirm,
    confirmDisabled,
    cancelLabel = 'Cancel',
    onCancel,
    emphasis = 'cancel',
}: PopupActionsProps) => (
    <Div className="popup__actions">
        <Button text={cancelLabel} variant={emphasis === 'cancel' ? 'primary' : 'secondary'} onClicked={onCancel}/>
        <Button text={confirmLabel} variant={emphasis === 'confirm' ? 'primary' : 'secondary'}
                onClicked={onConfirm} disabled={confirmDisabled}/>
    </Div>
);
