/** A file name's extension, lowercase, with no dot */
export const getFileExtension = (fileName: string): string => {
    const dot = fileName.lastIndexOf('.')
    return dot === -1 ? '' : fileName.slice(dot + 1).toLowerCase()
};
