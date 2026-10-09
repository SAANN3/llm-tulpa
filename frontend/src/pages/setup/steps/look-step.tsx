import {Div, Label} from '../../../components/primitives'
import {useTheme} from '../../../context/use-theme.ts'
import {AppearanceTab} from '../../settings/appearance-tab.tsx'
import type {StepDef, WizardContext} from '../types.ts'

/** The theme and the background, picked the way Settings → Appearance picks them */
export const useLookStep = ({persist}: WizardContext): StepDef => {
    const {themeName} = useTheme()

    return {
        key: 'look',
        group: 'Look',
        title: 'Look',
        canNext: true,
        onNext: async () => {
            await persist({theme: themeName})
            return true
        },
        body: (
            <Div className="setup__step">
                <Label className="setup__lead" text="Pick a look. You can change it any time in Settings → Appearance."/>
                <Div className="setup__look">
                    <AppearanceTab controls={false}/>
                </Div>
            </Div>
        ),
    }
};
