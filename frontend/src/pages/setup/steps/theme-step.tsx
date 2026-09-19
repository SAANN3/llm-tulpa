import {Div, Label} from '../../../components/primitives'
import {ThemePreview} from '../../../components/theme-preview.tsx'
import {useTheme} from '../../../context/use-theme.ts'
import type {StepDef, WizardContext} from '../types.ts'

export const useThemeStep = ({persist}: WizardContext): StepDef => {
    const {themeName} = useTheme()

    return {
        key: 'theme',
        title: 'Theme',
        canNext: true,
        onNext: async () => {
            await persist({theme: themeName})
            return true
        },
        body: (
            <Div className="setup__step">
                <Label className="setup__lead" text="Pick a look."/>
                <ThemePreview/>
            </Div>
        ),
    }
};
