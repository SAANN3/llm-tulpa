/** What the audio worklet (`pcm-capture.ts`) sends the page: a chunk of audio as 16 kHz mono 16-bit samples and the loudest
 * sample in it, or, between chunks, only the loudest sample since the last message (`pcm` is missing then) */
export interface PcmCaptureMessage {
    pcm?: ArrayBuffer
    /** The loudest sample, 0 to 1 */
    peak: number
}

/** What the page sends the worklet to have the part-filled chunk handed over now */
export const PCM_FLUSH = 'flush'
