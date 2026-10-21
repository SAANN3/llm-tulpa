import {Div, Label} from '../../../components/primitives'
import {VoiceModels} from '../../../components/voice-models.tsx'
import {useAuth} from '../../../context/use-auth.ts'
import {useSttStatus} from '../../../hooks/use-stt-status.ts'

/** The voice step's content: read when the step comes on screen, since the owner's account may be made just before it */
export const VoiceStepBody = () => {
    const {user} = useAuth()
    const {status, refresh} = useSttStatus()
    return (
        <Div className="setup__step">
            <Label className="setup__lead" text="Voice input lets you speak to the model instead of typing: a microphone button next to the message box. It needs a speech model, a download of a few hundred megabytes that runs on the CPU. It is optional: skip it now and install one later in Settings → Voice."/>
            {status
                ? <VoiceModels status={status} canManage={user?.role === 'owner'} onChanged={() => void refresh()}/>
                : <Label variant="secondary" text="Reading the models…"/>}
            <Label variant="secondary" text="A model keeps downloading if you go on to the next step."/>
        </Div>
    )
};
