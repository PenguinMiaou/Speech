import { describe, expect, it } from 'vitest'
import { version } from '../../../package.json'
import { WHATS_NEW, compareVersions, versionsToShow } from '../whatsNew'

describe('whatsNew', () => {
  it('has notes for the version in package.json, in Chinese and English', () => {
    expect(WHATS_NEW[version]?.zh.length).toBeGreaterThan(0)
    expect(WHATS_NEW[version]?.en.length).toBe(WHATS_NEW[version]?.zh.length)
  })

  it('compares versions as numbers', () => {
    expect(compareVersions('1.2.13', '1.2.9')).toBeGreaterThan(0)
    expect(compareVersions('1.2.9', '1.2.13')).toBeLessThan(0)
    expect(compareVersions('1.3.0', '1.2.99')).toBeGreaterThan(0)
    expect(compareVersions('1.2.12', '1.2.12')).toBe(0)
  })

  it('shows every version after the one last seen, newest first', () => {
    expect(versionsToShow('1.2.13', '1.2.11')).toEqual(['1.2.13', '1.2.12'])
    expect(versionsToShow('1.2.13', '1.2.12')).toEqual(['1.2.13'])
  })

  it('shows nothing once the current version was seen, or after a downgrade', () => {
    expect(versionsToShow('1.2.13', '1.2.13')).toEqual([])
    expect(versionsToShow('1.2.12', '1.2.13')).toEqual([])
  })

  it('shows only the current version when nothing was ever recorded', () => {
    expect(versionsToShow('1.2.13', null)).toEqual(['1.2.13'])
  })
})
