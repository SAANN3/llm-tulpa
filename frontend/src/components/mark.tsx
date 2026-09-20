import {Loader} from 'pixelarticons/react'

export interface MarkProps {
    spinning: boolean
    size?: number
}

/** The app's loading indicator, spins while loading and sits still otherwise */
export const Mark = ({spinning, size = 60}: MarkProps) => (
    <span data-mark data-spinning={spinning} style={{color: 'var(--color-primary)', display: 'inline-flex'}}>
      <Loader width={size} height={size}/>
    </span>
);
