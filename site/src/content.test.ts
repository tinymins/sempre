import { describe, expect, it } from 'vitest'
import { copy, resolveInitialLocale } from './content'

describe('homepage localization', () => {
  it('uses an explicit saved locale', () => {
    expect(resolveInitialLocale('en', 'zh-CN')).toBe('en')
    expect(resolveInitialLocale('zh-CN', 'en-US')).toBe('zh-CN')
  })

  it('falls back to the browser language', () => {
    expect(resolveInitialLocale(null, 'zh-Hans-CN')).toBe('zh-CN')
    expect(resolveInitialLocale(null, 'ja-JP')).toBe('en')
  })

  it('keeps both locales structurally identical', () => {
    expect(Object.keys(copy['zh-CN']).sort()).toEqual(Object.keys(copy.en).sort())
  })
})
