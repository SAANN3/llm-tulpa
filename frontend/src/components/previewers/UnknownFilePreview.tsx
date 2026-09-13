import { Div, Label } from '../primitives'
import type { PreviewerProps } from './types'
import { useTextContent } from './useTextContent'

/** Fraction of sampled characters that look like binary noise (a control character
 * outside common whitespace, or the replacement character left by a failed UTF-8
 * decode) above which content is treated as binary rather than text. */
const BINARY_THRESHOLD = 0.01

/** How much of the fetched content to sample for the binary check — no need to scan
 * a huge file just to answer "is this text", and a large file's `content` may itself
 * already be server-truncated (see `useTextContent`/the download route) anyway. */
const SNIFF_SAMPLE_CHARS = 4_000

function looksBinary(sample: string): boolean {
  if (sample.length === 0) return false

  let suspicious = 0
  for (const ch of sample) {
    const code = ch.codePointAt(0) ?? 0
    const isControl = code < 32 && ch !== '\n' && ch !== '\r' && ch !== '\t'
    if (isControl || code === 0xfffd) suspicious += 1
  }
  return suspicious / sample.length > BINARY_THRESHOLD
}

function unknownFileNotice(file: PreviewerProps['file']) {
  return (
    <Div className="vbox center" style={{ gap: 6, padding: 24, minWidth: 220 }}>
      <Label text="Unknown file type" style={{ fontSize: 14, fontWeight: 600 }} />
      <Label variant="secondary" text={file.file_name} style={{ fontSize: 12 }} />
    </Div>
  )
}

/** Fallback for a file extension nothing in `registry.ts` recognizes — fetches its
 * content the same way `TextPreview` does and, if it doesn't look like binary data,
 * shows it as plain text instead of giving up outright. Extensionless config files,
 * source in a language `CodePreview` doesn't have an entry for, and the like all land
 * here rather than "unknown file type" — which is still exactly what shows if the
 * content genuinely does look binary. */
export default function UnknownFilePreview({ file }: PreviewerProps) {
  const { content, failed } = useTextContent(file)

  if (failed) return unknownFileNotice(file)

  if (content == null) {
    return (
      <Div style={{ padding: 20 }}>
        <Label variant="secondary" text="Loading…" />
      </Div>
    )
  }

  if (looksBinary(content.slice(0, SNIFF_SAMPLE_CHARS))) return unknownFileNotice(file)

  return (
    <Div style={{ padding: 12 }}>
      <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word', fontFamily: 'monospace', fontSize: 13 }}>
        {content}
      </pre>
    </Div>
  )
}
