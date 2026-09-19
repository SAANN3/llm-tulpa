import {useState} from 'react'
import {Div, Input, Label} from '../../../components/primitives'
import {validateTimezone} from '../../../utils/validate-timezone.ts'
import type {StepDef, WizardContext} from '../types.ts'

const browserTimezoneOffsetHours = (): number => -new Date().getTimezoneOffset() / 60;

/** UTC-offset entry, prefilled from the browser. Returns the raw text for the owner step's save. */
export const useTimezoneStep = ({persist}: WizardContext): { step: StepDef; timezoneText: string } => {
    const [timezoneText, setTimezoneText] = useState(String(browserTimezoneOffsetHours()))
    const tz = validateTimezone(timezoneText)

    return {
        timezoneText,
        step: {
            key: 'timezone',
            title: 'Timezone',
            canNext: tz.valid,
            onNext: async () => {
                await persist({timezone: Number(timezoneText)})
                return true
            },
            body: (
                <Div className="setup__step">
                    <Div className="field">
                        <Label className="field__label" text="Timezone"/>
                        <Div className="field__control">
                            <Input className="field__input--tz" text={timezoneText} onChanged={setTimezoneText}
                                   placeholder="UTC offset"/>
                            {tz.echo ? <Label variant="secondary" text={tz.echo}/> : null}
                        </Div>
                        <Label variant="secondary" className={`field__help${tz.valid ? '' : ' field__help--accent'}`}
                               text={tz.message}/>
                    </Div>
                </Div>
            ),
        },
    }
};
