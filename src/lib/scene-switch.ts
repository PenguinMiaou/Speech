import type { ActiveScene, CustomScene } from '../stores/appStore'

/** The scene the switch-scene hotkey moves to: each of the user's scenes in turn, then none. */
export function nextActiveScene(
  scenes: CustomScene[],
  active: ActiveScene | null,
): ActiveScene | null {
  const at = active?.source === 'custom' ? scenes.findIndex((scene) => scene.id === active.id) : -1
  const next = scenes[at + 1]
  if (!next) return null
  return {
    id: next.id,
    source: 'custom',
    name: next.name,
    prompt_template: next.prompt_template,
  }
}
