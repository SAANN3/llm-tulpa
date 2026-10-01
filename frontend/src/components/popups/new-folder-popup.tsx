import {CreateFolderForm, type CreateFolderFormProps} from '../create-folder-form.tsx'
import {Popup} from './base/popup.tsx'

export interface NewFolderPopupProps extends Omit<CreateFolderFormProps, 'onCancel'> {
    open: boolean
    onClose: () => void
}

/** Name a new folder, directly or by describing it */
export const NewFolderPopup = ({open, onClose, createFolder, onCreated}: NewFolderPopupProps) => (
    <Popup open={open} onClose={onClose} title="New folder">
        <CreateFolderForm createFolder={createFolder} onCreated={onCreated} onCancel={onClose}/>
    </Popup>
);
