import { render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { beforeEach, describe, expect, it } from 'vitest'
import { App } from './App'
import { I18nProvider } from './lib/i18n'
import { SessionProvider } from './lib/session'

describe('App', () => {
  beforeEach(() => {
    sessionStorage.clear()
    localStorage.setItem('sempre.locale', 'en')
  })

  it('starts with the actual login workflow', () => {
    render(<QueryClientProvider client={new QueryClient()}><I18nProvider><SessionProvider><App /></SessionProvider></I18nProvider></QueryClientProvider>)
    expect(screen.getByRole('heading', { name: 'Sempre' })).toBeInTheDocument()
    expect(screen.getByLabelText('Sempre address')).toHaveValue(window.location.origin)
    expect(screen.getByRole('button', { name: /Connect/ })).toBeInTheDocument()
  })
})
