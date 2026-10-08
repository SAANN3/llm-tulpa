import {useState} from 'react'
import {useNavigate} from 'react-router-dom'
import type {LaunchProfile} from '../../api/profiles/types'
import {OllamaModelList} from '../ollama-model-list.tsx'
import {ProfilePicker} from '../profile-picker.tsx'
import {Button, Div} from '../primitives'
import {Popup} from './base/popup.tsx'

export interface ChooseModelPopupProps {
    open: boolean
    /** The provider of the current selection, which decides the tab the popup opens on */
    provider: string
    /** The Ollama model that is selected, when the selection is an Ollama one */
    selected?: string | null
    /** The launch profile that is selected, when the selection is a backend-run model */
    selectedProfileId?: number | null
    onSelect: (name: string) => void
    onSelectProfile: (profile: LaunchProfile) => void
    onClose: () => void
}

const TABS = [
    {id: 'llama-cpp', label: 'llama.cpp'},
    {id: 'ollama', label: 'Ollama'},
]

// Its own component, mounted only while open, so the tab starts from the current provider each time the popup opens
const ChooseModelDialog = ({provider, selected, selectedProfileId = null, onSelect, onSelectProfile, onClose}: Omit<ChooseModelPopupProps, 'open'>) => {
    const navigate = useNavigate()
    const [tab, setTab] = useState(provider === 'ollama' ? 'ollama' : 'llama-cpp')

    return (
        <Popup open onClose={onClose} title="Choose model" width={460} tabs={TABS} activeTab={tab} onTab={setTab} actions={[]}>
            {tab === 'ollama' ? (
                <OllamaModelList selected={provider === 'ollama' ? selected : null} onSelect={onSelect}/>
            ) : (
                <ProfilePicker selectedProfileId={selectedProfileId} onSelect={onSelectProfile}/>
            )}
            <Div className="popup__actions popup__actions--stacked">
                <Button variant="secondary" text="Manage models" onClicked={() => {
                    onClose()
                    navigate('/models')
                }}/>
                <Button variant="primary" text="Close" onClicked={onClose}/>
            </Div>
        </Popup>
    )
}

/** Pick a model: one tab per provider, the same kind of list in each. Adding, downloading and tuning
 * models happens on the Models page, one button away. */
export const ChooseModelPopup = ({open, ...rest}: ChooseModelPopupProps) => (open ? <ChooseModelDialog {...rest}/> : null);
