import { describe, expect, it } from 'vitest'
import { normalizeTheme, resolveTheme } from './theme'

describe('homepage theme', () => {
  it('normalizes saved preferences', () => {
    expect(normalizeTheme('light')).toBe('light')
    expect(normalizeTheme('dark')).toBe('dark')
    expect(normalizeTheme('system')).toBe('system')
    expect(normalizeTheme('invalid')).toBe('system')
    expect(normalizeTheme(null)).toBe('system')
  })

  it('resolves system and manual preferences', () => {
    expect(resolveTheme('system', false)).toBe('light')
    expect(resolveTheme('system', true)).toBe('dark')
    expect(resolveTheme('light', true)).toBe('light')
    expect(resolveTheme('dark', false)).toBe('dark')
  })
})
