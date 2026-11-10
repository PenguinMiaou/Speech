/** The names the local speech models are shown by. */
export const LOCAL_MODEL_NAMES: Record<string, string> = {
  'qwen3-asr-1.7b-q8': 'Qwen3 1.7B',
  'qwen3-asr-0.6b-q8': 'Qwen3 0.6B',
  'qwen3-asr-1.7b-f16': 'Qwen3 1.7B F16',
  'moss-transcribe-diarize-q8': 'MOSS Transcribe',
  'cohere-transcribe-q8': 'Cohere Transcribe',
  'voxtral-mini-4b-q8': 'Voxtral Mini 4B',
  'nemotron-3.5-0.6b-q8': 'Nemotron 0.6B',
}

export function localModelName(id: string): string {
  return LOCAL_MODEL_NAMES[id] ?? id
}
