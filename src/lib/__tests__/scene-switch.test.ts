import { describe, expect, it } from 'vitest'
import { nextActiveScene } from '../scene-switch'

const scene = (id: string) => ({
  id,
  name: `Scene ${id}`,
  description: '',
  prompt_template: `prompt ${id}`,
  created_at: '',
  updated_at: '',
})
const active = (id: string) => ({
  id,
  source: 'custom' as const,
  name: `Scene ${id}`,
  prompt_template: `prompt ${id}`,
})

describe('nextActiveScene', () => {
  const scenes = [scene('a'), scene('b')]

  it('goes from no scene to the first, through each, and back to none', () => {
    expect(nextActiveScene(scenes, null)).toEqual(active('a'))
    expect(nextActiveScene(scenes, active('a'))).toEqual(active('b'))
    expect(nextActiveScene(scenes, active('b'))).toBeNull()
  })

  it('starts over from the first when the active scene is not one of the user’s own', () => {
    const builtin = { ...active('x'), source: 'builtin' as const }
    expect(nextActiveScene(scenes, builtin)).toEqual(active('a'))
  })

  it('has nowhere to go without scenes', () => {
    expect(nextActiveScene([], null)).toBeNull()
  })
})
