import {useState} from 'react'
import {Div, Label, Select} from '../../../components/primitives'
import {SettingsRow, TimeFormatChoice} from '../../../components/settings-fields.tsx'
import {useTheme} from '../../../context/use-theme.ts'
import type {TimeFormat} from '../../../utils/time-format.ts'
import type {StepDef, WizardContext} from '../types.ts'

export const LANGUAGES: Record<string, string> = {English: 'en'}

// Whole hours, plus the half and three-quarter hours places really use
const OFFSETS = [...Array.from({length: 27}, (_, i) => i - 12), -9.5, -3.5, 3.5, 4.5, 5.5, 5.75, 6.5, 8.75, 9.5, 10.5, 12.75, 13.75]

const browserOffset = (): number => -new Date().getTimezoneOffset() / 60

/** `UTC+5:30`, `UTC-3`, `UTC` */
const offsetName = (offset: number): string => {
    if (offset === 0) return 'UTC'
    const minutes = Math.round(Math.abs(offset) * 60)
    const tail = minutes % 60 ? `:${String(minutes % 60).padStart(2, '0')}` : ''
    return `UTC${offset < 0 ? '-' : '+'}${Math.floor(minutes / 60)}${tail}`
}

/** The time of day it is now at `offset`, without seconds */
const nowAt = (offset: number, format: TimeFormat): string => {
    const there = new Date(Date.now() + offset * 3_600_000)
    const hours = there.getUTCHours()
    const minutes = String(there.getUTCMinutes()).padStart(2, '0')
    if (format === '24h') return `${String(hours).padStart(2, '0')}:${minutes}`
    return `${hours % 12 || 12}:${minutes} ${hours < 12 ? 'AM' : 'PM'}`
}

/** Language, timezone and time format. Nothing is saved for an account until one exists, so the account step saves
 * the language and timezone picked here once it has created it; the time format is kept in this browser at once. */
export const useBasicsStep = ({persist}: WizardContext): { step: StepDef; language: string; timezone: number } => {
    const {timeFormat} = useTheme()
    const [language, setLanguage] = useState('English')
    const [timezone, setTimezone] = useState(browserOffset)

    const offsets = [...new Set([...OFFSETS, browserOffset()])].sort((a, b) => a - b)
    const label = (offset: number) => `${offsetName(offset)} (now ${nowAt(offset, timeFormat)})`
    const labels = offsets.map(label)

    return {
        language,
        timezone,
        step: {
            key: 'basics',
            group: 'Basics',
            title: 'Basics',
            primaryLabel: 'Start',
            canNext: true,
            onNext: async () => {
                await persist({language: LANGUAGES[language], timezone})
                return true
            },
            body: (
                <Div className="setup__step">
                    <Label className="setup__lead" text="Let's get your local assistant set up."/>
                    <Div className="setup__rows">
                        <SettingsRow label="Language" help="More languages are coming later.">
                            <Select values={Object.keys(LANGUAGES)} selected={language} onChosen={setLanguage}/>
                        </SettingsRow>
                        <SettingsRow label="Timezone" help={`From your browser: ${offsetName(browserOffset())}. The model is told the time in it.`}>
                            <Select values={labels} selected={label(timezone)} onChosen={(chosen) => setTimezone(offsets[labels.indexOf(chosen)])}/>
                        </SettingsRow>
                        <SettingsRow label="Time format" help="How times of day are shown in the chats.">
                            <TimeFormatChoice/>
                        </SettingsRow>
                    </Div>
                </Div>
            ),
        },
    }
};
