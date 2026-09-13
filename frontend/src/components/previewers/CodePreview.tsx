import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter'
import { oneDark, oneLight } from 'react-syntax-highlighter/dist/esm/styles/prism'

import { useTheme } from '../../context/useTheme'
import { Div, Label } from '../primitives'
import type { PreviewerProps } from './types'
import { useTextContent } from './useTextContent'

/** Extension → the language name Prism expects, only where it actually differs from
 * the extension itself (Prism recognizes most extensions verbatim already — "py",
 * "json", "css", etc.). Unlisted extensions pass through unchanged. */
const LANGUAGE_ALIASES: Record<string, string> = {
  rs: 'rust',
  ts: 'typescript',
  js: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  py: 'python',
  sh: 'bash',
  zsh: 'bash',
  yml: 'yaml',
  kt: 'kotlin',
  kts: 'kotlin',
  rb: 'ruby',
  md: 'markdown',
  h: 'c',
  hpp: 'cpp',
  cc: 'cpp',
  hs: 'haskell',
  cs: 'csharp',
}

function getExtension(fileName: string): string {
  const dot = fileName.lastIndexOf('.')
  return dot === -1 ? '' : fileName.slice(dot + 1).toLowerCase()
}

function languageFor(fileName: string): string {
  const extension = getExtension(fileName)
  return LANGUAGE_ALIASES[extension] ?? extension
}

/** Syntax-highlighted source, via the full Prism language bundle (`react-syntax-
 * highlighter`'s `Prism` export) — using the "everything included" build rather than
 * per-language registration is fine here specifically because this whole component is
 * already lazy-loaded (see `registry.ts`): the bundle only downloads once a code file
 * is actually opened, so there's no per-language list to keep in sync with
 * `registry.ts`'s `CODE_EXTENSIONS`. Theme follows the app's own light/dark choice. */
export default function CodePreview({ file }: PreviewerProps) {
  const { content, failed } = useTextContent(file)
  const { themeName } = useTheme()

  if (failed) {
    return (
      <Div style={{ padding: 20 }}>
        <Label text="Couldn't load this file's content." />
      </Div>
    )
  }

  if (content == null) {
    return (
      <Div style={{ padding: 20 }}>
        <Label variant="secondary" text="Loading…" />
      </Div>
    )
  }

  return (
    <SyntaxHighlighter
      language={languageFor(file.file_name)}
      style={themeName === 'white' ? oneLight : oneDark}
      customStyle={{ margin: 0, fontSize: 13 }}
      showLineNumbers
    >
      {content}
    </SyntaxHighlighter>
  )
}
