import {useNavigate} from 'react-router-dom'
import {Div, Label} from '../../../components/primitives'
import type {StepDef} from '../types.ts'

export const useFinishStep = (): StepDef => {
    const navigate = useNavigate()

    return {
        key: 'social',
        title: 'All set',
        primaryLabel: 'Finish',
        canNext: true,
        onNext: async () => {
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
