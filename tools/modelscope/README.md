---
license: Apache License 2.0
---

# Speech model files

Copies of the model files that [Speech](https://github.com/PenguinMiaou/Speech) downloads, kept here so they can be fetched quickly from mainland China. Each file is byte for byte the file at its original address, which the path names:

- `hf/<user>/<repo>/<revision>/<file>` is `https://huggingface.co/<user>/<repo>/resolve/<revision>/<file>`
- `gh/<owner>/<repo>/<tag>/<file>` is `https://github.com/<owner>/<repo>/releases/download/<tag>/<file>`

The app checks every file against the SHA-256 it was built with, so a copy is used only when it is the same file.

## Files and their licences

| File | Origin | Licence |
| --- | --- | --- |
| Qwen3-ASR 1.7B and 0.6B, GGUF | Qwen (Alibaba), converted by handy-computer | Apache-2.0 |
| MOSS-Transcribe-Diarize, GGUF | OpenMOSS, converted by handy-computer | Apache-2.0 |
| cohere-transcribe-03-2026, GGUF | Cohere Labs, converted by handy-computer | Apache-2.0 |
| Voxtral-Mini-4B-Realtime-2602, GGUF | Mistral AI, converted by handy-computer | Apache-2.0 |
| nemotron-3.5-asr-streaming-0.6b, GGUF | NVIDIA, converted by handy-computer | OpenMDW-1.1 |
| Nemotron-3-Diarization, GGUF | NVIDIA, converted by the Speech project | OpenMDW-1.1, see `licenses/NOTICE-Nemotron-3-Diarization.txt` |
| SenseVoiceSmall (sherpa-onnx export) | FunAudioLLM / Alibaba, exported by csukuangfj | FunASR Model Open Source License 1.1 |
| pyannote segmentation-3.0 (sherpa-onnx export) | pyannote (CNRS), exported by csukuangfj | MIT |
| 3D-Speaker CAM++ speaker embedding | Alibaba 3D-Speaker, via k2-fsa/sherpa-onnx | Apache-2.0 |
| VoxLock enrolment and chunk models | arbazsiddiqui/voxlock | Apache-2.0 |

The licence texts are in `licenses/`. Names of the models and their authors are kept as published.
