import type {ThemedProps} from './primitives/types'

export interface ThinkingAnimationProps {
    isPlaying: boolean
}

/** Three dots that pulse in sequence while playing, then settle once done */
export const ThinkingAnimation = ({
    style,
    className,
    variant = 'secondary',
    isPlaying
}: ThemedProps<ThinkingAnimationProps>) => (
    <div style={style} className={className} data-variant={variant} data-thinking data-playing={isPlaying}>
        <span data-thinking-dot/>
        <span data-thinking-dot/>
        <span data-thinking-dot/>
    </div>
);
