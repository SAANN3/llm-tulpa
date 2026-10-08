import type {CSSProperties} from 'react'

export interface MarkProps {
    spinning: boolean
    size?: number
}

// pixelarticons' `loader`, cut into its eight arms (on its 24×24 grid) so each can light up on its own, clockwise from
// the top
const ARMS = [
    'M11 2h2v6h-2z',
    'M17 5h2v2h-2zM15 7h2v2h-2z',
    'M16 11h6v2h-6z',
    'M15 15h2v2h-2zM17 17h2v2h-2z',
    'M11 16h2v6h-2z',
    'M7 15h2v2H7zM5 17h2v2H5z',
    'M2 11h6v2H2z',
    'M7 7h2v2H7zM5 5h2v2H5z',
]

/** The app's loading indicator. While loading, its arms light up one after another around the circle, each fading
 * behind the next like a terminal spinner's; at rest every arm is lit. */
export const Mark = ({spinning, size = 60}: MarkProps) => (
    <svg data-mark data-spinning={spinning} width={size} height={size} viewBox="0 0 24 24" fill="currentColor"
         style={{color: 'var(--color-primary)'}} aria-hidden>
        {ARMS.map((d, i) => <path key={d} d={d} data-mark-arm style={{'--arm': i} as CSSProperties}/>)}
    </svg>
);
