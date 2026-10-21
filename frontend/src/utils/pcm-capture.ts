// Runs in the audio thread (voice input, src/hooks/use-speech-input.ts): takes what the microphone gives, at whatever
// rate the browser runs its audio at, and hands over 16 kHz mono 16-bit samples in chunks of half a second, and
// every 60 ms the loudest sample since the last one, for the level meter. Loaded as a worklet module by its URL
// (`?worker&url`), so it is built as a bundle of its own, with what it imports.
import {PCM_FLUSH, type PcmCaptureMessage} from './pcm-capture-messages.ts'

// The worklet scope's globals, which the DOM library does not have
declare class AudioWorkletProcessor {
    readonly port: MessagePort
}
declare function registerProcessor(name: string, processor: new () => AudioWorkletProcessor): void
declare const sampleRate: number

const TARGET_RATE = 16000
const CHUNK_SAMPLES = TARGET_RATE / 2
const LEVEL_SAMPLES = TARGET_RATE / 16

class PcmCapture extends AudioWorkletProcessor {
    // Input samples that go to one output sample; 1 when the browser already runs at 16 kHz
    private step = sampleRate / TARGET_RATE
    // Where the next output sample falls in the block being read, in input samples
    private position = 0
    private chunk = new Int16Array(CHUNK_SAMPLES)
    private filled = 0
    private peak = 0
    private levelPeak = 0
    private sinceLevel = 0

    constructor() {
        super()
        // The page asks for what is not yet a whole chunk when the user is done, so the last words are not left behind
        this.port.onmessage = (event: MessageEvent<string>) => {
            if (event.data !== PCM_FLUSH || this.filled === 0) return
            const rest = this.chunk.slice(0, this.filled)
            this.send({pcm: rest.buffer, peak: this.peak}, [rest.buffer])
            this.filled = 0
            this.peak = 0
        }
    }

    private send(message: PcmCaptureMessage, transfer: ArrayBuffer[] = []) {
        this.port.postMessage(message, transfer)
    }

    private push(sample: number) {
        const clamped = Math.max(-1, Math.min(1, sample))
        this.peak = Math.max(this.peak, Math.abs(clamped))
        this.levelPeak = Math.max(this.levelPeak, Math.abs(clamped))
        this.chunk[this.filled++] = clamped < 0 ? clamped * 0x8000 : clamped * 0x7fff
        if (this.filled === CHUNK_SAMPLES) {
            this.send({pcm: this.chunk.buffer as ArrayBuffer, peak: this.peak}, [this.chunk.buffer as ArrayBuffer])
            this.chunk = new Int16Array(CHUNK_SAMPLES)
            this.filled = 0
            this.peak = 0
        }
        // The chunk is half a second long; the meter and the voice graph want news more often than that
        if (++this.sinceLevel === LEVEL_SAMPLES) {
            this.send({peak: this.levelPeak})
            this.sinceLevel = 0
            this.levelPeak = 0
        }
    }

    process(inputs: Float32Array[][]): boolean {
        const input = inputs[0] && inputs[0][0]
        if (!input) return true
        // Linear interpolation between neighbours: enough for speech
        while (this.position < input.length) {
            const index = Math.floor(this.position)
            const fraction = this.position - index
            const next = index + 1 < input.length ? input[index + 1] : input[index]
            this.push(input[index] + (next - input[index]) * fraction)
            this.position += this.step
        }
        this.position -= input.length
        return true
    }
}

registerProcessor('pcm-capture', PcmCapture)
