import {useNavigate} from 'react-router-dom'
import {Button} from '../../components/primitives'
import {AutoConfirmField, MaxTurnStepsField, SettingsRow, SettingsSection, TrimOldThinkingField, UseToolsField} from '../../components/settings-fields.tsx'
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
            <UseToolsField enabled={draft.useTools} onToggle={(useTools) => onDraft({useTools})}/>
            <MaxTurnStepsField text={draft.maxTurnStepsText} onChanged={(maxTurnStepsText) => onDraft({maxTurnStepsText})}/>
        </SettingsSection>
    )
};
