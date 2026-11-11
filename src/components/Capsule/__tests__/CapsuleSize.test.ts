import { describe, expect, it } from 'vitest'
import { getCapsuleShellSize } from '../index'

describe('capsule size', () => {
  it('is only as wide as its contents while recording', () => {
    expect(getCapsuleShellSize('recording', false).width).toBe(126)
  })

  it('makes room for the language chip when translating', () => {
    expect(getCapsuleShellSize('recording', true).width).toBe(184)
  })
})
