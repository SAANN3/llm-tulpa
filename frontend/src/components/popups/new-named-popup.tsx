import {CreateNamedForm, type CreateNamedFormProps} from '../create-named-form.tsx'
import {Popup} from './base/popup.tsx'

export interface NewNamedPopupProps<T> extends Omit<CreateNamedFormProps<T>, 'onCancel'> {
    open: boolean
    title: string
    onClose: () => void
}

export const NewNamedPopup = <T,>({open, title, onClose, ...form}: NewNamedPopupProps<T>) => (
    <Popup open={open} onClose={onClose} title={title} width={480} actions={[]}>
        <CreateNamedForm {...form} onCancel={onClose}/>
    </Popup>
);
