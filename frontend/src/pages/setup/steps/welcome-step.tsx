import {useState} from 'react'
import {Div, Label, Select} from '../../../components/primitives'
import type {StepDef, WizardContext} from '../types.ts'

export const LANGUAGES: Record<string, string> = {English: 'en'}

/** Language pick. Returns the chosen language name for the steps that save it later. */
export const useWelcomeStep = ({persist}: WizardContext): { step: StepDef; language: string } => {
    const [language, setLanguage] = useState('English')

    return {
        language,
        step: {
            key: 'welcome',
            title: 'Welcome',
            primaryLabel: 'Start',
            canNext: true,
            onNext: async () => {
                await persist({language: LANGUAGES[language]})
                return true
            },
            body: (
                <Div className="setup__step">
                    <Label className="setup__lead" text="Let's get your local assistant set up."/>
                    <Div className="field">
                        <Label className="field__label" text="Language"/>
                        <Select className="setup__select" values={Object.keys(LANGUAGES)} selected={language}
                                onChosen={setLanguage}/>
                        <Label variant="secondary" className="field__help" text="More languages are coming later."/>
                    </Div>
                </Div>
            ),
        },
    }
};
