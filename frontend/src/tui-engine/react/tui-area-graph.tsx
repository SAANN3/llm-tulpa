import {useTuiWidget, type TuiComponentProps} from './use-tui-widget.ts'
import {TuiAreaGraphWidget, type AreaGraphOptions} from '../primitives/area-graph.ts'

export interface TuiAreaGraphProps extends TuiComponentProps, AreaGraphOptions {}

/** A graph of values over time as an area under a line (see `TuiAreaGraphWidget`) */
export const TuiAreaGraph = ({values, max, ...base}: TuiAreaGraphProps) => {
    const graph = {values, max}
    useTuiWidget(
        () => new TuiAreaGraphWidget({...base, ...graph}),
        base,
        (w) => w.configure(graph),
    )
    return null
};
