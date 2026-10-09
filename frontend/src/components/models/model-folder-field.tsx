import {useCallback, useEffect, useState} from 'react'
import {getModelFolder, setModelFolder, type ModelFolder} from '../../api/runtime/folder'
import {ModelFolderPopup} from '../popups/model-folder-popup.tsx'
import {Button, Div, Label} from '../primitives'
import {SettingsRow} from '../settings-fields.tsx'

export interface ModelFolderFieldProps {
    /** Called after the folder was changed, so the lists of model files can be read again */
    onChanged: () => void
}

/** The folder the model files live in, with a button to change it. Owner only. */
export const ModelFolderField = ({onChanged}: ModelFolderFieldProps) => {
    const [folder, setFolder] = useState<ModelFolder | null>(null)
    const [open, setOpen] = useState(false)

    const load = useCallback(() => {
        getModelFolder().then(setFolder).catch(() => setFolder(null))
    }, [])

    useEffect(load, [load])

    const choose = async (path: string) => {
        setFolder(await setModelFolder(path))
        onChanged()
    }

    return (
        <Div className="models__section">
            <SettingsRow label="Model folder" help={folder?.path ?? 'No folder chosen yet'}>
                <Button variant="secondary" text={folder?.path ? 'Change' : 'Choose'} onClicked={() => setOpen(true)}/>
            </SettingsRow>
            {folder?.path && folder.writable === false ? (
                <Label variant="secondary" className="models__error" text="Not writable: models run from it, but downloads can't be saved here."/>
            ) : null}
            {folder && folder.missing_models.length > 0 ? (
                <Label variant="secondary" className="models__error"
                       text={`Models you added earlier that are not in this folder, so they can't load from here: ${folder.missing_models.join(', ')}. Choose the folder they were in, or move the files here.`}/>
            ) : null}
            <ModelFolderPopup open={open} current={folder?.path ?? null} onChoose={choose} onClose={() => setOpen(false)}/>
        </Div>
    )
};
