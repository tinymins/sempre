import { lazy, Suspense } from 'react'
import { HashRouter, Navigate, Route, Routes } from 'react-router-dom'
import { Login } from './components/Login'
import { Spinner } from './components/ui'
import { useSession } from './lib/session'

const Shell = lazy(() => import('./components/Shell').then((module) => ({ default: module.Shell })))
const Overview = lazy(() => import('./pages/Overview').then((module) => ({ default: module.Overview })))
const CustomNodes = lazy(() => import('./pages/CustomNodes').then((module) => ({ default: module.CustomNodes })))
const Subscriptions = lazy(() => import('./pages/Subscriptions').then((module) => ({ default: module.Subscriptions })))
const Proxies = lazy(() => import('./pages/Proxies').then((module) => ({ default: module.Proxies })))
const Connections = lazy(() => import('./pages/Connections').then((module) => ({ default: module.Connections })))
const Rules = lazy(() => import('./pages/Rules').then((module) => ({ default: module.Rules })))
const Traffic = lazy(() => import('./pages/Traffic').then((module) => ({ default: module.Traffic })))
const Logs = lazy(() => import('./pages/Logs').then((module) => ({ default: module.Logs })))
const Management = lazy(() => import('./pages/Management').then((module) => ({ default: module.Management })))

export function App() {
  const { session } = useSession()
  if (!session) return <Login />
  return <Suspense fallback={<div className="grid min-h-screen place-items-center"><Spinner /></div>}><HashRouter><Shell><Routes><Route path="/" element={<Overview />} /><Route path="/custom-nodes" element={<CustomNodes />} /><Route path="/subscriptions" element={<Subscriptions />} /><Route path="/proxies" element={<Proxies />} /><Route path="/connections" element={<Connections />} /><Route path="/rules" element={<Rules />} /><Route path="/traffic" element={<Traffic />} /><Route path="/logs" element={<Logs />} /><Route path="/management" element={<Management />} /><Route path="*" element={<Navigate to="/" replace />} /></Routes></Shell></HashRouter></Suspense>
}
