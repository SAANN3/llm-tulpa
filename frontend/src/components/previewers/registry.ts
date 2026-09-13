import { lazy } from 'react'

import type { Previewer } from './types'

const TextPreview = lazy(() => import('./TextPreview'))
const CodePreview = lazy(() => import('./CodePreview'))
const PdfPreview = lazy(() => import('./PdfPreview'))
const HtmlPreview = lazy(() => import('./HtmlPreview'))
const DocxPreview = lazy(() => import('./DocxPreview'))
const XlsxPreview = lazy(() => import('./XlsxPreview'))

/** Extensions (lowercase, no dot) recognized as source code — get `CodePreview`'s
 * syntax highlighting rather than `TextPreview`'s plain rendering. Not exhaustive by
 * design; anything missing here just falls back to plain text (still readable, just
 * not colored) rather than "unknown file type" — only extensions genuinely unrelated
 * to text (images already have their own attachment kind, `.pdf`, office formats,
 * ...) get their own entry below or fall through to unknown. Add to this list as more
 * come up. */
const CODE_EXTENSIONS = [
  'py', 'rs', 'ts', 'tsx', 'js', 'jsx', 'mjs', 'cjs', 'go', 'c', 'h', 'cpp', 'hpp', 'cc',
  'java', 'kt', 'kts', 'swift', 'rb', 'php', 'cs', 'json', 'yaml', 'yml', 'toml', 'ini',
  'css', 'scss', 'less', 'sql', 'sh', 'bash', 'zsh', 'xml', 'md', 'lua', 'r', 'scala',
  'hs', 'elm', 'clj', 'vue', 'svelte', 'graphql', 'proto', 'dockerfile', 'diff', 'patch',
]

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
