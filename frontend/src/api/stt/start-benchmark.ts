import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {SttBenchmark} from './types'

/** Starts measuring how fast every installed model reads, at several thread counts, on a recording (16-bit little-endian
 * mono at 16 kHz, 3 to 15 seconds; owner only); watch it with `getSttStatus` */
export const startSttBenchmark = async (audio: ArrayBuffer): Promise<SttBenchmark> => {
    const {data} = await axios.post<SttBenchmark>(`${BACKEND_URL}/api/stt/benchmark`, audio, {
        headers: {'Content-Type': 'application/octet-stream'},
    })
    return data
};
