import { lazy, Suspense } from 'react'
import { useQuery } from '@tanstack/react-query'
import { HashRouter, Navigate, Route, Routes } from 'react-router-dom'
import { Login } from './components/Login'
import { Spinner } from './components/ui'
import { useSession } from './lib/session'
import { api } from './lib/api'
import type { NetworkSettingsResponse } from './lib/types'
import { ThemeProvider } from './lib/theme'
import { NetworkTest } from './pages/NetworkTest'

const Shell = lazy(() => import('./components/Shell').then((module) => ({ default: module.Shell })))
const Overview = lazy(() => import('./pages/Overview').then((module) => ({ default: module.Overview })))
const CustomNodes = lazy(() => import('./pages/CustomNodes').then((module) => ({ default: module.CustomNodes })))
const Subscriptions = lazy(() => import('./pages/Subscriptions').then((module) => ({ default: module.Subscriptions })))
const Proxies = lazy(() => import('./pages/Proxies').then((module) => ({ default: module.Proxies })))
const Connections = lazy(() => import('./pages/Connections').then((module) => ({ default: module.Connections })))
const Rules = lazy(() => import('./pages/Rules').then((module) => ({ default: module.Rules })))
const RoutingRules = lazy(() => import('./pages/RoutingRules').then((module) => ({ default: module.RoutingRules })))
const Dns = lazy(() => import('./pages/Dns').then((module) => ({ default: module.Dns })))
const Traffic = lazy(() => import('./pages/Traffic').then((module) => ({ default: module.Traffic })))
const Logs = lazy(() => import('./pages/Logs').then((module) => ({ default: module.Logs })))
const Gateway = lazy(() => import('./pages/Gateway').then((module) => ({ default: module.Gateway })))
const Tunnels = lazy(() => import('./pages/Tunnels').then((module) => ({ default: module.Tunnels })))
const Management = lazy(() => import('./pages/Management').then((module) => ({ default: module.Management })))
const RuntimeStatus = lazy(() => import('./pages/RuntimeStatus').then((module) => ({ default: module.RuntimeStatus })))
const AcmeShowcase = import.meta.env.DEV ? lazy(() => import('./pages/AcmeShowcase').then((module) => ({ default: module.AcmeShowcase }))) : null
const ServerApp = lazy(() => import('./features/server/ServerApp').then((module) => ({ default: module.ServerApp })))
const serverBuild = import.meta.env.VITE_SEMPRE_SERVER === 'true'

export function App() {
  return <ThemeProvider><AppContent /></ThemeProvider>
}

export function AppContent({ serverMode = serverBuild }: { serverMode?: boolean }) {
  const { session } = useSession()
  if (serverMode || window.location.hash.startsWith('#/server')) return <Suspense fallback={<div className="grid min-h-screen place-items-center"><Spinner /></div>}><ServerApp /></Suspense>
  const isDevShowcase = Boolean(AcmeShowcase) && window.location.hash.startsWith('#/components')
  if (!session && !isDevShowcase) return <Login />
  return <Suspense fallback={<div className="grid min-h-screen place-items-center"><Spinner /></div>}><HashRouter><Shell><Routes><Route path="/" element={<Overview />} /><Route path="/custom-nodes" element={<CustomNodes />} /><Route path="/subscriptions" element={<Subscriptions />} /><Route path="/tunnels" element={<Tunnels />} /><Route path="/proxies" element={<Proxies />} /><Route path="/connections" element={<Connections />} /><Route path="/routing-rules" element={<RoutingRules />} /><Route path="/rules" element={<Rules />} /><Route path="/dns" element={<Dns />} /><Route path="/traffic" element={<Traffic />} /><Route path="/logs" element={<Logs />} /><Route path="/network-test" element={<NetworkTest />} /><Route path="/runtime-status" element={<RuntimeStatus />} /><Route path="/gateway" element={<GatewayRoute />} /><Route path="/management" element={<Management />} />{AcmeShowcase ? <Route path="/components" element={<AcmeShowcase />} /> : null}<Route path="*" element={<Navigate to="/" replace />} /></Routes></Shell></HashRouter></Suspense>
}

function GatewayRoute() {
  const { session } = useSession()
  const network = useQuery({ queryKey: ['network', 'settings'], queryFn: () => api<NetworkSettingsResponse>(session!, '/network/settings'), enabled: Boolean(session) })
  if (network.isLoading) return <div className="grid min-h-64 place-items-center"><Spinner /></div>
  return network.data?.settings.mode === 'gateway' ? <Gateway /> : <Navigate to="/" replace />
}
