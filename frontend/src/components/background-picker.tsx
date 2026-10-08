import '../styles/background-picker.scss'
import {backgroundEffects, findBackground, type BackgroundId} from '../backgrounds'
import {useTheme} from '../context/use-theme.ts'
import {Div, Label, RadioButton, Slider} from './primitives'

/** The choices the list starts with, before the animated backgrounds */
const PLAIN: { id: BackgroundId; name: string }[] = [{id: 'off', name: 'Off'}, {id: 'dots', name: 'Dots'}]

const GROUPS = [...new Set(backgroundEffects.map((e) => e.group))]

interface TuningProps {
    label: string
    value: number
    onChange: (value: number) => void
}

const Tuning = ({label, value, onChange}: TuningProps) => (
    <Div className="background-picker__slider">
        <Label text={label}/>
        <Slider value={value} onChanged={onChange} min={0.3} max={2} step={0.05}/>
        <Label variant="secondary" className="background-picker__value" text={value.toFixed(2)}/>
    </Div>
)

/** What is behind the pages, applied at once and kept in this browser, like the theme. A grouped list for now; a
 * previewer of its own is planned with the Settings tabs. */
export const BackgroundPicker = () => {
    const {background, setBackground} = useTheme()
    const chosen = findBackground(background.id)

    const row = (id: BackgroundId, name: string, title?: string) => (
        <Div key={id} className="background-picker__row" onClick={() => setBackground({id})}>
            <RadioButton name="background-picker" value={id} checked={background.id === id} onChanged={() => setBackground({id})}/>
            <span title={title}>{name}</span>
        </Div>
    )

    return (
        <Div className="field">
            <Label className="field__label" text="Background"/>
            <Div className="background-picker__list">
                {PLAIN.map((p) => row(p.id, p.name))}
                {GROUPS.map((group) => (
                    <Div key={group} className="background-picker__group">
                        <Label variant="secondary" className="background-picker__group-name" text={group}/>
                        {backgroundEffects.filter((e) => e.group === group).map((e) => row(e.id, e.name, e.description))}
                    </Div>
                ))}
            </Div>
            {chosen ? (
                <>
                    <Label variant="secondary" className="field__help" text={chosen.description}/>
                    <Tuning label="Brightness" value={background.strength} onChange={(strength) => setBackground({strength})}/>
                    <Tuning label="Speed" value={background.speed} onChange={(speed) => setBackground({speed})}/>
                </>
            ) : null}
        </Div>
    )
};
