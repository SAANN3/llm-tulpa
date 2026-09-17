import type {ComponentType} from 'react'

import type {FileOut} from '../../api/files/types'

export interface PreviewerProps {
    file: FileOut
}

export type Previewer = ComponentType<PreviewerProps>
