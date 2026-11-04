import {useNavigate} from 'react-router-dom'
import {Button} from '../../components/primitives'
import {AutoConfirmField, MaxTurnStepsField, SettingsRow, SettingsSection, SuggestNextField, TrimOldThinkingField} from '../../components/settings-fields.tsx'
import type {SettingsDraftProps} from './draft.ts'

/** How the model works in the user's chats */
export const BehaviourTab = ({draft, onDraft}: SettingsDraftProps) => {
    const navigate = useNavigate()
    return (
        <SettingsSection title="Behaviour">
            <SettingsRow label="System prompt" help="How the model behaves in your chats">
                <Button variant="secondary" text="Customize" onClicked={() => navigate('/settings/system-prompt')}/>
            </SettingsRow>
            <AutoConfirmField enabled={draft.autoConfirm} onToggle={(autoConfirm) => onDraft({autoConfirm})}/>
            <TrimOldThinkingField enabled={draft.trimOldThinking} onToggle={(trimOldThinking) => onDraft({trimOldThinking})}/>
            <SuggestNextField enabled={draft.suggestNext} onToggle={(suggestNext) => onDraft({suggestNext})}/>
            <SettingsRow label="Tools in new chats" help="Which tools a new chat sends the model. A chat can choose its own afterwards.">
                <Button variant="secondary" text="Choose" onClicked={() => navigate('/settings/tools')}/>
            </SettingsRow>
            <MaxTurnStepsField text={draft.maxTurnStepsText} onChanged={(maxTurnStepsText) => onDraft({maxTurnStepsText})}/>
        </SettingsSection>
    )
};
