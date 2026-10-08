import {useKeyLayer} from './use-key-layer.ts'

/** Calls `onClose` when Escape is pressed while this overlay is the topmost open one */
export const useEscapeToClose = (open: boolean, onClose: () => void) =>
    useKeyLayer(open, (e) => {
        if (e.key !== 'Escape') return false
        onClose()
        return true
    });
