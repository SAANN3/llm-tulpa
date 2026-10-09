import {useState} from 'react'
import {useNavigate, useParams} from 'react-router-dom'
import '../styles/settings.scss'
import {Frame} from '../components/frame.tsx'
import {Button, Div} from '../components/primitives'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useSettings} from '../context/use-settings.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {useGoBack} from '../hooks/use-go-back.ts'
import {parseMaxTurnSteps} from '../utils/parse-max-turn-steps.ts'
import {validateTimezone} from '../utils/validate-timezone.ts'
import {AppearanceTab} from './settings/appearance-tab.tsx'
import {BehaviourTab} from './settings/behaviour-tab.tsx'
import {draftOf, type SettingsDraft} from './settings/draft.ts'
import {ModelsTab} from './settings/models-tab.tsx'
import {ProfileTab} from './settings/profile-tab.tsx'

const TABS = [
    {id: 'behaviour', label: 'Behaviour'},
    {id: 'appearance', label: 'Appearance'},
    {id: 'profile', label: 'Profile'},
    {id: 'models', label: 'Models & keys'},
]

/** The user's settings in four tabs, the tab in the address (`/settings/appearance`). Behaviour and Profile are
 * saved together with Save; Appearance (kept in this browser) and Models & keys apply as they are changed. */
const Settings = () => {
    useDocumentTitle('Settings')
    const navigate = useNavigate()
    const {tab: tabSlug} = useParams()
    const tab = TABS.find((t) => t.id === tabSlug)?.id ?? TABS[0].id
    const {settings, setSettings} = useSettings()
    const [draft, setDraft] = useState<SettingsDraft>(() => draftOf(settings))
    const onDraft = (change: Partial<SettingsDraft>) => setDraft((now) => ({...now, ...change}))
    const onBack = useGoBack()

    const tz = validateTimezone(draft.timezoneText)
    const steps = parseMaxTurnSteps(draft.maxTurnStepsText)
    const saveDisabled = draft.name.trim().length === 0 || !tz.valid || !steps.valid

    const onSave = async () => {
        if (saveDisabled) return
        await setSettings({
            name: draft.name.trim(),
            timezone: Number(draft.timezoneText),
            notifications_enabled: draft.notifications,
            auto_confirm: draft.autoConfirm,
            trim_old_thinking: draft.trimOldThinking,
            use_tools: draft.useTools,
            max_turn_steps: steps.value,
        })
        onBack()
    }

    return (
        <Div className="page center vbox settings">
            <TypewriterLabel className="settings__title" text="[ Settings ]" charIntervalMs={30}/>
            <Frame className="settings__panel" bodyClassName="settings__body" tabs={TABS} activeTab={tab}
                   onTab={(id) => navigate(`/settings/${id}`, {replace: true})}
                   actions={[]} onEscape={onBack} escapeLabel="back">
                <Div className={`settings__content${tab === 'appearance' ? ' settings__content--fill' : ''}`}>
                    {tab === 'behaviour' ? <BehaviourTab draft={draft} onDraft={onDraft}/> : null}
                    {tab === 'appearance' ? <AppearanceTab/> : null}
                    {tab === 'profile' ? <ProfileTab draft={draft} onDraft={onDraft}/> : null}
                    {tab === 'models' ? <ModelsTab/> : null}
                </Div>
                <Div className="settings__actions">
                    <Button className="settings__action" variant="secondary" text="Back" onClicked={onBack}/>
                    <Button className="settings__action" text="Save" onClicked={onSave} disabled={saveDisabled}/>
                </Div>
            </Frame>
        </Div>
    )
};

export default Settings
