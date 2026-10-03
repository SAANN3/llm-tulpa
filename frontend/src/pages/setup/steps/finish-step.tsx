import {useNavigate} from 'react-router-dom'
import {completeSetup} from '../../../api/runtime/setup'
import {Div, Label} from '../../../components/primitives'
import {useAuth} from '../../../context/use-auth.ts'
import {useSetup} from '../../../context/use-setup.ts'
import type {StepDef} from '../types.ts'

export const useFinishStep = (): StepDef => {
    const navigate = useNavigate()
    const {token} = useAuth()
    const {refresh} = useSetup()

    return {
        key: 'social',
        title: 'All set',
        primaryLabel: 'Finish',
        canNext: true,
        onNext: async () => {
            // Best effort: a failure only means the update prompt shows again
            if (token) await completeSetup().then(refresh).catch(() => undefined)
            navigate('/', {replace: true})
            return true
        },
        body: (
            <Div className="setup__step">
                <Label className="setup__lead" text="You're ready to go."/>
                <Label variant="secondary" className="field__help field__help--wide"
                       text="Connect messaging integrations any time from the Plugins page."/>
            </Div>
        ),
    }
};
