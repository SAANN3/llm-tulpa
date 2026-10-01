import {Label} from '../primitives'

/** What a stats view shows until it has data: a loading line, or a failure */
export const StatsStatus = ({failed}: { failed?: boolean }) =>
    failed ? <Label text="Couldn't load this."/> : <Label variant="secondary" text="Loading…"/>;
