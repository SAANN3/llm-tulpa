import type {PathedFile} from '../api/plugins/tab-upload.ts'

/** Every file of a drop, a dropped folder's files included with their paths inside it. The entry API is the only way a
 * browser tells a folder from a file in a drop; without it (or with no entries) the plain list of files is all there is. */
export const readDroppedFiles = async (data: DataTransfer): Promise<PathedFile[]> => {
    const entries = Array.from(data.items ?? [])
        .map((item) => item.webkitGetAsEntry?.())
        .filter((entry): entry is FileSystemEntry => !!entry)
    if (entries.length === 0) return Array.from(data.files).map((file) => ({path: file.name, file}))
    const out: PathedFile[] = []
    for (const entry of entries) await walk(entry, '', out)
    return out
}

const walk = async (entry: FileSystemEntry, parent: string, out: PathedFile[]): Promise<void> => {
    const path = parent ? `${parent}/${entry.name}` : entry.name
    if (entry.isFile) {
        const file = await new Promise<File>((resolve, reject) => (entry as FileSystemFileEntry).file(resolve, reject))
        out.push({path, file})
        return
    }
    const reader = (entry as FileSystemDirectoryEntry).createReader()
    // A reader hands over its entries in batches, until an empty one
    for (;;) {
        const batch = await new Promise<FileSystemEntry[]>((resolve, reject) => reader.readEntries(resolve, reject))
        if (batch.length === 0) return
        for (const child of batch) await walk(child, path, out)
    }
}
