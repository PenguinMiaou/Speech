<p align="center">
  <strong>English</strong> | <a href="README_zh.md">中文</a>
</p>

<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="128" height="128" alt="OpenTypeless logo" />
</p>

<h1 align="center">OpenTypeless, local speech recognition</h1>

Hold a hotkey, speak, and the text is typed into the app you are using. Speech recognition runs on your computer. Only the recognized text goes to a language model for cleanup, and you choose that model and supply its key.

This is a fork of [OpenTypeless](https://github.com/tover0314-w/opentypeless) (MIT). The hosted service, accounts and paid plans of the original are removed.

## What it does

- **Dictation into any app.** Hold or tap the hotkey, speak, release. The text is inserted at the cursor.
- **Cleanup by a language model.** Filler words, false starts and spoken self-corrections are removed, and several points become a list. Polish can be switched off; then the recognized text is inserted as it is.
- **Rewrite selected text by voice.** Select text, speak an instruction, and the selection is replaced.
- **Ask a question by voice** and get a short answer in a floating note.
- **Translation** of what you say into one or more target languages.
- **Scenes and per-app writing style**, with your own prompt templates.
- **A personal dictionary** with file import.

## What this fork adds

- **Speech recognition on the machine.** Models are downloaded from the settings page with one click and checked against a SHA-256 before use. Recognition runs in a separate worker process that exits when idle.
- **Context for the recognizer.** The newest 30 dictionary words, your last few dictations in the same app (kept in memory for 10 minutes) and up to 200 characters before the cursor are given to the local recognizer. Text read from the screen is never sent to a cloud service.
- **Learning from your edits.** After a dictation the app watches the text field for up to a minute. A word you correct by hand is added to your dictionary together with what it was misheard as. Password fields are not read, and the text of the field is compared in memory and not stored.
- **Layout habits per app.** If you keep deleting the final period or joining lines in one app, polish stops adding them there. Only counts are stored.
- **Only my voice.** An optional filter keeps the enrolled speaker and removes other talkers before recognition. It uses the VoxLock model (Apache-2.0) through sherpa-onnx and needs a voice sample of at least 3 seconds, which stays on the machine.
- **Microphone handling.** Pick a microphone, switch automatically when the chosen one is silent, and mute the computer's sound output while recording (macOS).
- **A fallback polish model**, tried once when the main model fails.

## Local models

| Model                        | Size    | Notes                                           |
| ---------------------------- | ------- | ----------------------------------------------- |
| Qwen3-ASR 1.7B (Q8)          | 2.19 GB | Recommended. Best for mixed Chinese and English |
| Qwen3-ASR 0.6B (Q8)          | 850 MB  | Smaller and faster, weaker on English terms     |
| Qwen3-ASR 1.7B (F16)         | 4.09 GB |                                                 |
| MOSS Transcribe Diarize (Q8) | 987 MB  |                                                 |
| Cohere Transcribe (Q8)       | 2.41 GB |                                                 |
| Voxtral Mini 4B (Q8)         | 4.73 GB |                                                 |
| Nemotron 3.5 0.6B (Q8)       | 751 MB  |                                                 |

On 76 held-out recordings of real dictation, mostly Chinese with English terms, Qwen3-ASR 1.7B scored a character similarity of 0.83 against the reference text and recalled 91 of 122 technical terms. On an Apple Silicon laptop a 6 second recording is recognized in about 0.7 seconds and a 12 second one in about 1.3 seconds.

Cloud recognition from the original app still works with your own key: Deepgram, AssemblyAI, Aliyun Qwen3 ASR, Volcengine Doubao, GLM ASR, OpenAI Whisper, Groq Whisper, SiliconFlow, any OpenAI-compatible Whisper server, and Apple Speech on macOS.

## Polish models

Polish works with any service that speaks OpenAI Chat Completions, OpenAI Responses or Anthropic Messages. Presets are included for Zhipu, DeepSeek, SiliconFlow, OpenAI, Gemini, Moonshot, Doubao, Qwen, Groq, Claude, OpenRouter and Ollama. With Ollama nothing leaves the machine.

## What leaves the machine

| Data                                                | Where it goes                                                         |
| --------------------------------------------------- | --------------------------------------------------------------------- |
| Audio                                               | Nowhere with a local model. To the speech service you chose otherwise |
| Recognized text                                     | To the polish model you configured. Nowhere when polish is off        |
| Text before the cursor, text read back from a field | Nowhere. Used by the local recognizer and for learning, in memory     |
| Voice sample, dictionary, history, settings         | Stored on the machine                                                 |
| API keys                                            | In the system keychain or credential manager                          |

There is no account and no telemetry. At launch the app asks GitHub whether a newer release exists; this can be switched off under Settings, About.

## Updates

The app checks for a newer release when it starts and says so in a bar at the top of the window. Settings, About can switch that off and has a button to check by hand. On Windows the app downloads only the program files that changed, checks each against the SHA-256 the release lists and replaces them, showing the progress. The new version takes effect when you press Restart. Models, settings and history are not touched. On macOS the button opens the release page, where the `.dmg` is downloaded and the app dragged onto Applications.

The home page shows the tokens the polish model reports using: today, the last 30 days and in all. The counts are kept on this machine and stay hidden when the provider reports none.

## Platforms

| Platform               | State                                                                                                                                                         |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS, Apple Silicon   | In daily use. Local recognition uses Metal                                                                                                                    |
| Windows 10 and 11, x64 | Installer and no-install archive in the releases. Local recognition runs on the CPU. Reading text fields back and muting sound output are not implemented yet |
| Linux                  | The original app supports it. The additions of this fork have not been run there                                                                              |

## Build from source

Requirements: Node 20, Rust 1.82 or newer, CMake, and the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) for your system.

```bash
npm ci
npm run tauri dev      # run in development
npm run tauri build    # build the app
```

The build compiles the speech worker (`stt-worker/`) first and bundles it with the app. It downloads a prebuilt sherpa-onnx library for the voice filter.

On macOS, sign the build with a stable identity so that the Accessibility and Microphone permissions survive a rebuild:

```bash
APPLE_SIGNING_IDENTITY="<your identity>" npm run tauri build -- --bundles app
```

Tests:

```bash
npm test
cargo test --manifest-path src-tauri/Cargo.toml --lib
(cd stt-worker && cargo test)
```

## First run

1. Install it. On macOS open the `.dmg` and drag the app onto Applications; a newer version is installed the same way and keeps its permissions. On Windows run the installer.
2. Open the app and follow the guide: grant Microphone and Accessibility, then download a local model.
3. Enter the base URL and key of a polish model under Settings, AI Polish, and press Test.
4. Read the sample sentence in the last step of the guide. With local recognition this reading is kept as your voice sample.
5. Hold the hotkey in any app and speak.

## License

MIT, as the original project. See [LICENSE](LICENSE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
