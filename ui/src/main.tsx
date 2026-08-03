import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { App } from './App'
import { I18nProvider } from './lib/i18n'
import { SessionProvider } from './lib/session'
import './index.css'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 2000, retry: 1, refetchOnWindowFocus: true },
    mutations: { retry: false },
  },
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <I18nProvider><SessionProvider><App /></SessionProvider></I18nProvider>
    </QueryClientProvider>
  </StrictMode>,
)
