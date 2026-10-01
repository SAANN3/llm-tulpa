import {Div, Label} from '../primitives'

export interface StatCardProps {
    label: string
    value: string
    /** A smaller line under the value */
    hint?: string
}

/** One headline number with its label */
export const StatCard = ({label, value, hint}: StatCardProps) => (
    <Div className="stat-card">
        <Label variant="secondary" text={label}/>
        <Label className="stat-card__value" text={value}/>
        {hint ? <Label variant="secondary" className="stat-card__hint" text={hint}/> : null}
    </Div>
);
