import {lazy} from 'react'

import type {Previewer} from './types'

const TextPreview = lazy(() => import('./text-preview.tsx'))
const CodePreview = lazy(() => import('./code-preview.tsx'))
const PdfPreview = lazy(() => import('./pdf-preview.tsx'))
const HtmlPreview = lazy(() => import('./html-preview.tsx'))
const DocxPreview = lazy(() => import('./docx-preview.tsx'))
const XlsxPreview = lazy(() => import('./xlsx-preview.tsx'))
const ImagePreview = lazy(() => import('./image-preview.tsx'))
const VideoPreview = lazy(() => import('./video-preview.tsx'))
const AudioPreview = lazy(() => import('./audio-preview.tsx'))

const CODE_EXTENSIONS = [
    'py', 'rs', 'ts', 'tsx', 'js', 'jsx', 'mjs', 'cjs', 'go', 'c', 'h', 'cpp', 'hpp', 'cc',
    'java', 'kt', 'kts', 'swift', 'rb', 'php', 'cs', 'json', 'yaml', 'yml', 'toml', 'ini',
    'css', 'scss', 'less', 'sql', 'sh', 'bash', 'zsh', 'xml', 'md', 'lua', 'r', 'scala',
    'hs', 'elm', 'clj', 'vue', 'svelte', 'graphql', 'proto', 'dockerfile', 'diff', 'patch',
]

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
    csv: XlsxPreview,
}

for (const extension of IMAGE_EXTENSIONS) EXTENSION_PREVIEWERS[extension] = ImagePreview
for (const extension of VIDEO_EXTENSIONS) EXTENSION_PREVIEWERS[extension] = VideoPreview
for (const extension of AUDIO_EXTENSIONS) EXTENSION_PREVIEWERS[extension] = AudioPreview

for (const extension of CODE_EXTENSIONS) {
    EXTENSION_PREVIEWERS[extension] = CodePreview
}

export const getPreviewer = (extension: string): Previewer | null => EXTENSION_PREVIEWERS[extension] ?? null;

export const getMediaKind = (extension: string): 'image' | 'video' | null => {
    if (IMAGE_EXTENSIONS.includes(extension)) return 'image'
    if (VIDEO_EXTENSIONS.includes(extension)) return 'video'
    return null
};
