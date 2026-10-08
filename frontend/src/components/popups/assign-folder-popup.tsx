import {FolderPicker} from '../folder-picker.tsx'
import {Popup} from './base/popup.tsx'

export interface AssignFolderPopupProps {
    open: boolean
    /** The folder the chat is in now, `null` for none */
    selected: number | null
    onSelect: (folderId: number | null) => void
    onClose: () => void
}

/** Pick, clear or create the folder a chat belongs to */
export const AssignFolderPopup = ({open, selected, onSelect, onClose}: AssignFolderPopupProps) => (
    <Popup open={open} onClose={onClose} title="Assign folder" actions={[]}>
        <FolderPicker selected={selected} onSelect={onSelect}/>
    </Popup>
);
