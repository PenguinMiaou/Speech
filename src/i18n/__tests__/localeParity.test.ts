import { describe, expect, it } from 'vitest'
import de from '../locales/de.json'
import en from '../locales/en.json'
import es from '../locales/es.json'
import fr from '../locales/fr.json'
import itLocale from '../locales/it.json'
import ja from '../locales/ja.json'
import ko from '../locales/ko.json'
import pt from '../locales/pt.json'
import ru from '../locales/ru.json'
import zh from '../locales/zh.json'

const locales = { de, en, es, fr, it: itLocale, ja, ko, pt, ru, zh }

function leafKeys(value: unknown, prefix = ''): string[] {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return prefix ? [prefix] : []
  }

  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    leafKeys(child, prefix ? `${prefix}.${key}` : key),
  )
}

/** A plural form of a message, which only the languages that have that form carry. */
const PLURAL = /_(zero|one|two|few|many)$/

describe('locale message coverage', () => {
  it('keeps every locale aligned with English leaf keys', () => {
    const expectedKeys = leafKeys(en)
      .filter((key) => !PLURAL.test(key))
      .sort()

    for (const [locale, messages] of Object.entries(locales)) {
      const keys = leafKeys(messages)
      expect(keys.filter((key) => !PLURAL.test(key)).sort(), locale).toEqual(expectedKeys)
      // A plural form stands beside the message it is a form of.
      for (const key of keys.filter((key) => PLURAL.test(key))) {
        expect(keys, `${locale} ${key}`).toContain(key.replace(PLURAL, ''))
      }
    }
  })
})
