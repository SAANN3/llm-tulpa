/** A file name's extension, lowercase, no dot — `''` if it has none. Shared by
 * `AttachmentPreview` (picking a previewer) and `Attachment` (deciding whether its own
 * small thumbnail chip can show a real image/video instead of a generic file icon), so
 * both agree on what counts as e.g. `.JPG` vs `.jpg`. */
export function getFileExtension(fileName: string): string {
  const dot = fileName.lastIndexOf('.')
  return dot === -1 ? '' : fileName.slice(dot + 1).toLowerCase()
}
