import {useState} from 'react'
import {useNavigate, useParams} from 'react-router-dom'

import '../styles/models.scss'
import {HfPanel} from '../components/models/hf-panel.tsx'
import {HardwarePanel} from '../components/models/hardware-panel.tsx'
import {ModelsPanel} from '../components/models/models-panel.tsx'
import {Frame} from '../components/frame.tsx'
import {ModelPicker} from '../components/model-picker.tsx'
import {OllamaAddressField} from '../components/ollama-address-field.tsx'
import {PresetsPanel} from '../components/models/presets-panel.tsx'
import {RuntimePanel} from '../components/models/runtime-panel.tsx'
import {Button, Div, Label} from '../components/primitives'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useAuth} from '../context/use-auth.ts'
import {useSettings} from '../context/use-settings.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {useModelsData} from '../hooks/use-models-data.ts'
import {useRuntime} from '../hooks/use-runtime.ts'

const TABS = ['Models', 'Download', 'Presets', 'Hardware', 'Ollama'] as const

/** The models the backend runs itself: what is loaded, how each model is launched, the user's own
 * sampling presets and what the hardware looks like to llama.cpp. The tab lives in the address
 * (`/models/presets`), so a refresh or a shared link lands on the same view. */
const Models = () => {
    useDocumentTitle('Models')
    const navigate = useNavigate()
    const {tab: tabSlug} = useParams()
    const {user} = useAuth()
    const {settings} = useSettings()
    const isOwner = user?.role === 'owner'
    const tab = TABS.find((name) => name.toLowerCase() === tabSlug) ?? TABS[0]

    // Bumped when Ollama's address changes, so its list of models is read again from the new one
    const [ollamaKey, setOllamaKey] = useState(0)
    const {status, refresh} = useRuntime()
    const {models, profiles, files, error, reload} = useModelsData(isOwner)
    const changed = () => {
        void refresh()
        void reload()
    }

    const loaded = profiles.find((p) => p.id === status?.profile_id)
    // What Load starts: the first launch profile of the user's default model
    const defaultProfileId = models.find((m) => m.file === settings?.active_model)?.profile_ids[0] ?? null
    // The model whose launch profile is loaded, when one is
    const loadedModel = models.find((m) => loaded != null && m.profile_ids.includes(loaded.id)) ?? null

    return (
        <Div className="page center vbox models">
            <TypewriterLabel className="models__title" text="[ Models ]" charIntervalMs={30}/>
            <Frame className="models__panel" bodyClassName="models__body"
                   tabs={TABS.map((name) => ({id: name, label: name}))} activeTab={tab}
                   onTab={(id) => navigate(`/models/${id.toLowerCase()}`, {replace: true})}
                   actions={[]} onEscape={() => navigate('/')} escapeLabel="back">
                    <Div className="models__scroll">
                    <RuntimePanel status={status} profileName={loaded?.name ?? null} isOwner={isOwner}
                                  defaultProfileId={defaultProfileId} onChanged={changed}/>

                    {error ? <Div className="models__error">{error}</Div> : null}
                    {tab === 'Models' ? (
                        <ModelsPanel status={status} models={models} profiles={profiles} files={files} isOwner={isOwner}
                                     onChanged={changed}/>
                    ) : null}
                    {tab === 'Download' && isOwner ? <HfPanel onDownloaded={changed}/> : null}
                    {tab === 'Download' && !isOwner ? <Label variant="secondary" text="Only the owner downloads models."/> : null}
                    {tab === 'Presets' ? <PresetsPanel models={models} loadedModelId={loadedModel?.id ?? null}/> : null}
                    {tab === 'Hardware' ? <HardwarePanel status={status}/> : null}
                    {tab === 'Ollama' ? (
                        <>
                            {isOwner ? <OllamaAddressField onSaved={() => setOllamaKey((k) => k + 1)}/> : null}
                            <ModelPicker key={ollamaKey}/>
                        </>
                    ) : null}

                    </Div>

                    <Button variant="secondary" text="Back" onClicked={() => navigate('/')}/>
            </Frame>
        </Div>
    )
};

export default Models
