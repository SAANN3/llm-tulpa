export interface FolderOut {
    id: number
    name: string
    created_at: string
    updated_at: string
}

export interface FoldersOut {
    folders: FolderOut[]
    total: number
}

export type GetFoldersResponse = FolderOut | FoldersOut
