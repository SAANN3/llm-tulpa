import type {StatsRange} from '../../api/stats/types'
import type {HeatmapView} from '../../components/stats/heatmap.tsx'

export interface RangeOption {
    /** What the range is called in the page's address */
    slug: string
    label: string
    /** What the API is asked for. A module-level constant on purpose: its identity is what tells the
     * data hooks the range hasn't changed. */
    query: StatsRange
    heatmap: HeatmapView
}

export const RANGES: RangeOption[] = [
    {slug: '7d', label: '7 days', query: {days: 7}, heatmap: 'hours'},
    {slug: '30d', label: '30 days', query: {days: 30}, heatmap: 'blocks'},
    {slug: '3m', label: '3 months', query: {months: 3}, heatmap: 'months'},
];

export const DEFAULT_RANGE = RANGES[1];
