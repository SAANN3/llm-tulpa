import {useNavigate, useParams} from 'react-router-dom'

import '../styles/models.scss'
import {HfPanel} from '../components/models/hf-panel.tsx'
import {HardwarePanel} from '../components/models/hardware-panel.tsx'
import {ModelsPanel} from '../components/models/models-panel.tsx'
import {ModelPicker} from '../components/model-picker.tsx'
import {PresetsPanel} from '../components/models/presets-panel.tsx'
import {RuntimePanel} from '../components/models/runtime-panel.tsx'
import {Button, Div, Label} from '../components/primitives'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useAuth} from '../context/use-auth.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {useModelsData} from '../hooks/use-models-data.ts'
import {useRuntime} from '../hooks/use-runtime.ts'

const TABS = ['Models', 'Download', 'Presets', 'Hardware', 'Ollama'] as const
type Tab = (typeof TABS)[number]

/** The models the backend runs itself: what is loaded, how each model is launched, the user's own
 * sampling presets and what the hardware looks like to llama.cpp. The tab lives in the address
 * (`/models/presets`), so a refresh or a shared link lands on the same view. */
const Models = () => {
    useDocumentTitle('Models')
    const navigate = useNavigate()
    const {tab: tabSlug} = useParams()
    const {user} = useAuth()
    const isOwner = user?.role === 'owner'
    const tab = TABS.find((name) => name.toLowerCase() === tabSlug) ?? TABS[0]

    const {status, refresh} = useRuntime()
    const {models, profiles, files, error, reload} = useModelsData(isOwner)
    const changed = () => {
        void refresh()
        void reload()
    }

    const loaded = profiles.find((p) => p.id === status?.profile_id)

    return (
        <Div className="page center vbox models">
            <TypewriterLabel className="models__title" text="[ Models ]" charIntervalMs={30}/>
            <Div className="dos-frame models__panel">
                <span className="dos-frame__title">{tab}</span>
                <Div className="dos-frame__body models__body">
                    <RuntimePanel status={status} profileName={loaded?.name ?? null} isOwner={isOwner} onChanged={changed}/>

                    <Div className="models__tabs">
                        {TABS.map((name: Tab) => (
                            <Button key={name} variant={name === tab ? 'primary' : 'secondary'} text={name}
                                    onClicked={() => navigate(`/models/${name.toLowerCase()}`, {replace: true})}/>
                        ))}
                    </Div>

                    {error ? <Div className="models__error">{error}</Div> : null}
                    {tab === 'Models' ? (
                        <ModelsPanel status={status} models={models} profiles={profiles} files={files} isOwner={isOwner}
                                     onChanged={changed}/>
                    ) : null}
                    {tab === 'Download' && isOwner ? <HfPanel onDownloaded={changed}/> : null}
                    {tab === 'Download' && !isOwner ? <Label variant="secondary" text="Only the owner downloads models."/> : null}
                    {tab === 'Presets' ? <PresetsPanel models={models}/> : null}
                    {tab === 'Hardware' ? <HardwarePanel status={status}/> : null}
                    {tab === 'Ollama' ? <ModelPicker/> : null}

                    <Button variant="secondary" text="Back" onClicked={() => navigate('/')}/>
                </Div>
            </Div>
        </Div>
    )
};

export default Models
