import {ModelPicker} from '../model-picker.tsx'
import {Popup} from './base/popup.tsx'

export interface ChooseModelPopupProps {
    open: boolean
    selected?: string | null
    onSelect: (name: string) => void
    onClose: () => void
}

/** Pick one of the installed models */
export const ChooseModelPopup = ({open, selected, onSelect, onClose}: ChooseModelPopupProps) => (
    <Popup open={open} onClose={onClose} title="Choose model" width={460}>
        <ModelPicker selected={selected} onSelect={onSelect}/>
    </Popup>
);
