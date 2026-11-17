// Writes update-manifest.json: the program files of a release with their size and SHA-256.
// The app reads it to download only the files that changed.
// It is signed with the release key from UPDATE_SIGNING_KEY (an Ed25519 key in PEM), and the
// app uses a manifest only with that signature, wherever it was downloaded from.
// Usage: node scripts/update-manifest.mjs <directory with the release files> <version> [--unsigned]
import { createHash, createPrivateKey, sign } from 'node:crypto'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const [directory, version, unsigned] = process.argv.slice(2)
const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const built = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8')).version
if (version !== built) {
  throw new Error(`the tag says ${version} but the app was built as ${built}`)
}

// Platform -> the name next to the running program -> the release file that holds it.
const PROGRAM_FILES = {
  'windows-x64': {
    'opentypeless.exe': 'opentypeless-windows-x64.exe',
    'ot-stt-worker.exe': 'ot-stt-worker-windows-x64.exe',
  },
  // On macOS the whole signed app is replaced, so it is one file: the app, packed.
  'macos-arm64': {
    'Speech.app': 'Speech-macos-arm64.app.tar.gz',
  },
}

const files = {}
for (const [platform, entries] of Object.entries(PROGRAM_FILES)) {
  files[platform] = Object.entries(entries).map(([name, asset]) => {
    const content = readFileSync(join(directory, asset))
    return {
      name,
      asset,
      sha256: createHash('sha256').update(content).digest('hex'),
      size: content.length,
    }
  })
}

const manifest = Buffer.from(JSON.stringify({ version, files }, null, 2) + '\n')
writeFileSync(join(directory, 'update-manifest.json'), manifest)

if (unsigned !== '--unsigned') {
  const key = process.env.UPDATE_SIGNING_KEY
  if (!key) throw new Error('UPDATE_SIGNING_KEY is not set: the app refuses an unsigned manifest')
  const signature = sign(null, manifest, createPrivateKey(key)).toString('base64')
  writeFileSync(join(directory, 'update-manifest.json.sig'), signature + '\n')
}
