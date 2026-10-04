import {useRuntime} from '../hooks/use-runtime.ts'
import {Div, Label} from './primitives'

/** A line shown while the model server is loading a model or a request is waiting its turn for it, so a
 * reply that takes a minute isn't mistaken for a hang */
export const ModelStateBanner = ({watching = false}: { watching?: boolean }) => {
    const {status} = useRuntime(watching)
    if (status?.queued) {
        return (
            <Div className="chat__status">
                <Label variant="secondary"
                       text={`Waiting for the model${status.holders.length ? ` — ${status.holders.join(', ')} is using it` : ''}; your request goes next.`}/>
            </Div>
        )
    }
    if (status?.state !== 'starting') return null

    return (
        <Div className="chat__status">
            <Label variant="secondary"
                   text={`Loading ${status.model ?? 'the model'} — this can take a minute, and the reply follows once it is ready.`}/>
        </Div>
    )
};
