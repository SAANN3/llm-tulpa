import {ActiveModelField, DebugField, HfTokenField, SettingsSection} from '../../components/settings-fields.tsx'
import {useSettings} from '../../context/use-settings.ts'
import {useDebug} from '../../hooks/use-debug.ts'
import {setDebug} from '../../utils/debug.ts'

/** The model new chats start on, the keys for downloading models, and this browser's debug switch. Each applies at
 * once, apart from the Save of the other tabs. */
export const ModelsTab = () => {
    const {settings, setSettings} = useSettings()
    const debug = useDebug()
    return (
        <SettingsSection title="Models & keys">
            <ActiveModelField
                provider={settings?.llm_provider ?? 'ollama'}
                model={settings?.active_model ?? null}
                launchProfileId={settings?.launch_profile_id ?? null}
                onChosen={(llm_provider, active_model, launch_profile_id) => setSettings({llm_provider, active_model, launch_profile_id})}
            />
            <HfTokenField hasToken={settings?.has_hf_token ?? false} onSave={(hf_token) => setSettings({hf_token})}/>
            <DebugField enabled={debug} onToggle={setDebug}/>
        </SettingsSection>
    )
};
