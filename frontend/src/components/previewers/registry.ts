import { lazy } from 'react'

import type { Previewer } from './types'

const TextPreview = lazy(() => import('./TextPreview'))
const CodePreview = lazy(() => import('./CodePreview'))
const PdfPreview = lazy(() => import('./PdfPreview'))
const HtmlPreview = lazy(() => import('./HtmlPreview'))
const DocxPreview = lazy(() => import('./DocxPreview'))
const XlsxPreview = lazy(() => import('./XlsxPreview'))
const ImagePreview = lazy(() => import('./ImagePreview'))
const VideoPreview = lazy(() => import('./VideoPreview'))
const AudioPreview = lazy(() => import('./AudioPreview'))

/** Extensions (lowercase, no dot) recognized as source code — get `CodePreview`'s
 * syntax highlighting rather than `TextPreview`'s plain rendering. Not exhaustive by
 * design; anything missing here just falls back to plain text (still readable, just
 * not colored) rather than "unknown file type" — only extensions genuinely unrelated
 * to text (images, `.pdf`, office formats, ...) get their own entry below or fall
 * through to unknown. Add to this list as more come up. */
const CODE_EXTENSIONS = [
  'py', 'rs', 'ts', 'tsx', 'js', 'jsx', 'mjs', 'cjs', 'go', 'c', 'h', 'cpp', 'hpp', 'cc',
  'java', 'kt', 'kts', 'swift', 'rb', 'php', 'cs', 'json', 'yaml', 'yml', 'toml', 'ini',
  'css', 'scss', 'less', 'sql', 'sh', 'bash', 'zsh', 'xml', 'md', 'lua', 'r', 'scala',
  'hs', 'elm', 'clj', 'vue', 'svelte', 'graphql', 'proto', 'dockerfile', 'diff', 'patch',
]

/** Same idea, for `ImagePreview`/`VideoPreview`/`AudioPreview` below — this is
 * specifically about a `kind: 'file'` attachment (referenced by id, previewed via this
 * registry), a completely separate code path from `kind: 'image'` (an inline base64
 * image, e.g. one typed straight into the composer), which never goes through
 * `getPreviewer` at all. A model-attached image (`ui.attach_file` on a real image
 * path) or an uploaded one always lands here as a `file`, so without these entries it
 * fell through to `UnknownFilePreview`'s binary check — correctly binary, uselessly
 * "unknown file type" — instead of actually rendering. */
export const IMAGE_EXTENSIONS = ['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg', 'ico', 'avif']
export const VIDEO_EXTENSIONS = ['mp4', 'webm', 'mov', 'mkv', 'avi', 'm4v']
const AUDIO_EXTENSIONS = ['mp3', 'wav', 'ogg', 'flac', 'm4a', 'aac']

const EXTENSION_PREVIEWERS: Record<string, Previewer> = {
  txt: TextPreview,
  log: TextPreview,
  pdf: PdfPreview,
  html: HtmlPreview,
  htm: HtmlPreview,
  docx: DocxPreview,
  xlsx: XlsxPreview,
  xls: XlsxPreview,
  // SheetJS's `read` auto-detects CSV from raw bytes the same as it does xlsx/xls —
  // reusing `XlsxPreview` gets a real table view (with proper column alignment) for
  // free, rather than csv only ever showing as an unbroken wall of plain text.
  csv: XlsxPreview,
}

for (const extension of IMAGE_EXTENSIONS) EXTENSION_PREVIEWERS[extension] = ImagePreview
for (const extension of VIDEO_EXTENSIONS) EXTENSION_PREVIEWERS[extension] = VideoPreview
for (const extension of AUDIO_EXTENSIONS) EXTENSION_PREVIEWERS[extension] = AudioPreview

for (const extension of CODE_EXTENSIONS) {
  EXTENSION_PREVIEWERS[extension] = CodePreview
}

/** The previewer for a file extension (lowercase, no dot) — `null` if nothing here
 * recognizes it, which `AttachmentPreview` falls back to `UnknownFilePreview` for
 * (plain text if the content itself doesn't look binary, "unknown file type"
 * otherwise). `.pptx` is deliberately not mapped to anything of its own: there's no
 * good lightweight, maintained way to render slides client-side, so it lands on that
 * same fallback rather than a broken/half-working dedicated attempt. */
export function getPreviewer(extension: string): Previewer | null {
  return EXTENSION_PREVIEWERS[extension] ?? null
}

/** Whether a `file`-kind attachment's extension is an image/video — for `Attachment`'s
 * own small thumbnail chip (not this file's preview registry above) to decide whether
 * it can show the real thing instead of a generic file icon, the same way an inline
 * `kind: 'image'` attachment already does. */
export function getMediaKind(extension: string): 'image' | 'video' | null {
  if (IMAGE_EXTENSIONS.includes(extension)) return 'image'
  if (VIDEO_EXTENSIONS.includes(extension)) return 'video'
  return null
}
