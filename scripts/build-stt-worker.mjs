// Builds ot-stt-worker and places it where Tauri expects a sidecar binary.
import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')

const hostTriple = () =>
  execFileSync('rustc', ['-vV'], { encoding: 'utf8' })
    .split('\n')
    .find((line) => line.startsWith('host: '))
    .slice('host: '.length)
    .trim()

const target = process.env.TAURI_ENV_TARGET_TRIPLE || hostTriple()
const extension = target.includes('windows') ? '.exe' : ''

execFileSync(
  'cargo',
  [
    'build',
    '--release',
    '--manifest-path',
    join(root, 'stt-worker/Cargo.toml'),
    '--target',
    target,
  ],
  // Run from the worker's directory so that its .cargo/config.toml applies.
  { stdio: 'inherit', cwd: join(root, 'stt-worker') },
)

const destination = join(root, 'src-tauri/binaries')
mkdirSync(destination, { recursive: true })
copyFileSync(
  join(root, 'stt-worker/target', target, 'release', `ot-stt-worker${extension}`),
  join(destination, `ot-stt-worker-${target}${extension}`),
)
