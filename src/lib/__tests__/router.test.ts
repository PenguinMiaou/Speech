import { afterEach, describe, expect, it } from 'vitest'
import { parseHash } from '../router'

afterEach(() => {
  window.location.hash = ''
})

describe('desktop route parsing', () => {
  it('sends old account and upgrade links to the home page', () => {
    window.location.hash = '#/upgrade'
    expect(parseHash()).toBe('home')
    window.location.hash = '#/account'
    expect(parseHash()).toBe('home')
  })

  it('keeps direct recovery links inside Settings', () => {
    window.location.hash = '#/settings?pane=stt'
    expect(parseHash()).toBe('settings')
    window.location.hash = '#/settings?pane=llm'
    expect(parseHash()).toBe('settings')
  })
})
