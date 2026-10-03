import {useState} from 'react'
import {rebindChats} from '../../../api/runtime/setup'
import {Div, Label, ToggleSwitch} from '../../../components/primitives'
import {errorReason} from '../../../utils/error-reason.ts'
import type {StepDef} from '../types.ts'

/** Offers to move chats that run on an Ollama model onto the model just chosen; on by default */
export const useRebindStep = (profileId: number | null): StepDef => {
    const [move, setMove] = useState(true)
    const [note, setNote] = useState<string | null>(null)

    return {
        key: 'rebind',
        title: 'Existing chats',
        canNext: true,
        onNext: async () => {
            if (!move || profileId == null) return true
            try {
                await rebindChats(profileId)
                return true
            } catch (e) {
                setNote(errorReason(e, 'Could not move the chats.'))
                return false
            }
        },
        body: (
            <Div className="setup__step">
                <Label className="setup__lead" text="Move your existing chats to the new model?"/>
                <Div className="field__row">
                    <Label className="field__row-label" text="Move them"/>
                    <ToggleSwitch toggled={move} onToggled={setMove}/>
                </Div>
                <Label variant="secondary" className="field__help field__help--wide"
                       text="Chats keep all their messages either way. Left off, each chat stays on the model it was using and keeps working while Ollama is available."/>
                {note ? <Label className="model-picker__error" text={note}/> : null}
            </Div>
        ),
    }
};
