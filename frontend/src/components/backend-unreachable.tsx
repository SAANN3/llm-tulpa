import '../styles/backend-unreachable.scss'
import {BACKEND_URL} from '../config'
import {Frame} from './frame.tsx'
import {Button, Div, Label} from './primitives'

export interface BackendUnreachableProps {
    /** What went wrong, as the page found out */
    reason: string
    /** Asks again now, instead of at the next automatic try */
    onRetry: () => void
}

/**
 * Shown, over everything else, while the backend can't be reached: it is probably stopped or restarting, and
 * the page asks again every few seconds and carries on by itself once it answers. Without it a page that can't
 * ask the backend whether it is set up looks the same as a backend that isn't.
 */
export const BackendUnreachable = ({reason, onRetry}: BackendUnreachableProps) => (
    <Div className="page center vbox backend-unreachable">
        <Frame className="backend-unreachable__panel" bodyClassName="backend-unreachable__body" title="Can't reach the backend"
               actions={[{keys: ['Enter'], shown: 'enter', label: 'try now', run: onRetry}]}>
            <Label text={reason}/>
            <Label variant="secondary" text={`The page asks ${BACKEND_URL} again every few seconds and carries on when it answers.`}/>
            <Label variant="secondary" text="If it stays like this, the backend is not running, or something is in the way of its port: look at its log (for the Docker setup, `docker compose logs backend`)."/>
            <Div className="backend-unreachable__actions">
                <Button text="Try now" onClicked={onRetry}/>
            </Div>
        </Frame>
    </Div>
);
