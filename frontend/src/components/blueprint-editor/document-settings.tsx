import {Frame} from '../frame'
import {Div, Input, Label, Select} from '../primitives'
import {SavedVariables} from './saved-variables'
import type {BlueprintDocument, Schedule} from '../../api/blueprints/types'

interface Props {
    view: string
    name: string
    schedule: Schedule
    nextDue: string | null
    locked: boolean
    saved: BlueprintDocument | null
    variablesLocked: boolean
    onName: (name: string) => void
    onSchedule: (schedule: Schedule) => void
}

export const DocumentSettings = ({view, name, schedule, nextDue, locked, saved, variablesLocked, onName, onSchedule}: Props) => (
    <Div className="blueprint-editor__settings">
        <Frame title={view === 'schedule' ? 'Schedule' : 'Blueprint properties'} actions={[]} active={false}>
            <fieldset disabled={locked}>
                {view === 'general' ? (
                    <label className="blueprint-editor__field">
                        <span>Name</span>
                        <Input text={name} onChanged={onName}/>
                    </label>
                ) : (
                    <>
                        <label className="blueprint-editor__field">
                            <span>Trigger</span>
                            <Select values={['Manual', 'Interval', 'Daily']}
                                    selected={{manual: 'Manual', interval: 'Interval', daily: 'Daily'}[schedule.type]}
                                    onChosen={value => {
                                        if (value === 'Manual') onSchedule({type: 'manual'})
                                        else if (value === 'Interval') onSchedule({type: 'interval', seconds: 3600})
                                        else onSchedule({type: 'daily', time: '09:00', timezone: Intl.DateTimeFormat().resolvedOptions().timeZone})
                                    }}/>
                        </label>
                        {schedule.type === 'interval' ? (
                            <label className="blueprint-editor__field">
                                <span>Interval in seconds</span>
                                <Input type="number" text={String(schedule.seconds)}
                                       onChanged={value => onSchedule({...schedule, seconds: Number(value)})}/>
                            </label>
                        ) : null}
                        {schedule.type === 'daily' ? (
                            <>
                                <label className="blueprint-editor__field">
                                    <span>Time</span>
                                    <Input type="time" text={schedule.time} onChanged={time => onSchedule({...schedule, time})}/>
                                </label>
                                <label className="blueprint-editor__field">
                                    <span>Timezone</span>
                                    <Input text={schedule.timezone} onChanged={timezone => onSchedule({...schedule, timezone})}/>
                                </label>
                            </>
                        ) : null}
                        <Label variant="secondary" className="blueprint-editor__hint" text={schedule.type === 'manual'
                            ? 'Manual runs start from Runs.'
                            : nextDue
                                ? `Next run ${new Date(nextDue).toLocaleString()}. This is the saved schedule. Finished runs are in History.`
                                : 'No run is scheduled yet. Finished runs are in History.'}/>
                    </>
                )}
            </fieldset>
            {view === 'general' && saved ? (
                <SavedVariables key={saved.blueprint.id} saved={saved} locked={variablesLocked}/>
            ) : null}
        </Frame>
    </Div>
)
