import {useTuiWidget, type TuiComponentProps} from './use-tui-widget.ts'
import {TuiScannerBarWidget, type ScannerBarOptions} from '../primitives/scanner-bar.ts'

export interface TuiScannerBarProps extends TuiComponentProps, ScannerBarOptions {}

/** A scanner-style progress bar (see `TuiScannerBarWidget`). A bar that sweeps moves only on a canvas that draws
 * continuously. */
export const TuiScannerBar = ({value, indeterminate, fillMode, revealMode, showPercent, speed, borderStyle, seed, ...base}: TuiScannerBarProps) => {
    const bar = {value, indeterminate, fillMode, revealMode, showPercent, speed, borderStyle, seed}
    useTuiWidget(
        () => new TuiScannerBarWidget({...base, ...bar}),
        base,
        (w) => w.configure(bar),
    )
    return null
};
