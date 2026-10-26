import type { AddressProbe, BrowserReport, IceAddress } from './types'

export const BROWSER_ENDPOINTS = {
  ipv4: 'https://api.ipify.org?format=json',
  ipv6: 'https://api6.ipify.org?format=json',
  stun: 'stun:stun.l.google.com:19302',
}

export function normalizeIp(value: string): string | null {
  const address = value.trim().toLowerCase()
  if (/^\d+\.\d+\.\d+\.\d+$/.test(address)) {
    const parts = address.split('.')
    return parts.every((part) => String(Number(part)) === part && Number(part) <= 255) ? address : null
  }
  if (!/^[0-9a-f:.]+$/.test(address) || !address.includes(':')) return null
  try { return new URL(`http://[${address}]/`).hostname.slice(1, -1) } catch { return null }
}

function addressScope(address: string): IceAddress['scope'] {
  if (address.endsWith('.local')) return 'mdns'
  const ip = normalizeIp(address)
  if (!ip) return 'unknown'
  if (ip.includes(':')) {
    if (ip === '::' || ip === '::1' || /^(fc|fd|fe[89ab])/.test(ip)) return 'nonpublic'
    if (ip.startsWith('2001:db8:')) return 'nonpublic'
    // Other IPv6 scopes, including mapped addresses, need separate evidence.
    return /^[23]/.test(ip) ? 'public' : 'unknown'
  }
  const [a, b, c] = ip.split('.').map(Number)
  const reserved = a === 0 || a === 10 || a === 127 || a >= 224 || (a === 100 && b >= 64 && b <= 127)
    || (a === 169 && b === 254) || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168)
    || (a === 192 && b === 0 && (c === 0 || c === 2)) || (a === 198 && (b === 18 || b === 19 || (b === 51 && c === 100)))
    || (a === 203 && b === 0 && c === 113)
  return reserved ? 'nonpublic' : 'public'
}

async function addressProbe(family: 'ipv4' | 'ipv6', signal: AbortSignal, zh: boolean): Promise<AddressProbe> {
  const endpoint = BROWSER_ENDPOINTS[family]
  const controller = new AbortController()
  const abort = () => controller.abort()
  signal.addEventListener('abort', abort, { once: true })
  const timer = setTimeout(abort, 10000)
  if (signal.aborted) controller.abort()
  try {
    const response = await fetch(endpoint, { signal: controller.signal, credentials: 'omit', cache: 'no-store', referrerPolicy: 'no-referrer', redirect: 'error' })
    if (!response.ok) throw new Error(`HTTP ${response.status}`)
    const body = await response.json() as { ip?: unknown }
    const ip = typeof body.ip === 'string' ? normalizeIp(body.ip) : null
    if (!ip || (family === 'ipv6') !== ip.includes(':')) throw new Error(zh ? '返回地址类型不符' : 'Unexpected address family')
    return { endpoint, status: 'observed', ip, detail: zh ? '此 HTTPS 请求的浏览器出口；不能代表应用出口。' : 'Browser egress for this HTTPS request; not application egress.' }
  } catch (error) {
    return { endpoint, status: 'unknown', detail: signal.aborted ? (zh ? '已停止，覆盖不完整' : 'Stopped; coverage incomplete') : `${zh ? '未取得结果；可能受网络、CORS 或浏览器策略限制' : 'No result; network, CORS or browser policy may block this probe'}: ${error instanceof Error ? error.message : String(error)}` }
  } finally {
    clearTimeout(timer)
    signal.removeEventListener('abort', abort)
  }
}

async function webRtcProbe(signal: AbortSignal, zh: boolean): Promise<BrowserReport['webrtc']> {
  if (typeof RTCPeerConnection === 'undefined') return { status: 'unknown', candidates: [], detail: zh ? '浏览器不支持 WebRTC' : 'WebRTC is unavailable' }
  if (signal.aborted) return { status: 'unknown', candidates: [], detail: zh ? '已停止' : 'Stopped' }
  return new Promise((resolve) => {
    const candidates: IceAddress[] = []
    let peer: RTCPeerConnection | undefined
    let timer: ReturnType<typeof setTimeout> | undefined
    let ended = false
    let stunError = false
    const finish = (detail: string, complete: boolean) => {
      if (ended) return
      ended = true
      clearTimeout(timer)
      signal.removeEventListener('abort', abort)
      if (peer) {
        peer.onicecandidate = null
        peer.onicecandidateerror = null
        peer.close()
      }
      resolve({ status: complete && candidates.length > 0 ? 'observed' : 'unknown', candidates, detail })
    }
    const abort = () => finish(zh ? '已停止；已有候选地址只表示部分观测' : 'Stopped; collected candidates are partial evidence', false)
    signal.addEventListener('abort', abort, { once: true })
    try {
      peer = new RTCPeerConnection({ iceServers: [{ urls: BROWSER_ENDPOINTS.stun }], iceCandidatePoolSize: 0 })
      peer.createDataChannel('sempre-leak-check')
      peer.onicecandidateerror = () => { stunError = true }
      peer.onicecandidate = ({ candidate }) => {
        if (!candidate) {
          finish(stunError ? (zh ? '候选收集结束，STUN 曾失败；未观测的路径仍未知' : 'Gathering ended with a STUN error; unobserved paths remain unknown') : (zh ? '候选收集结束；没有候选地址不等于没有泄漏' : 'Gathering ended; absence of candidates does not prove absence of leaks'), !stunError)
          return
        }
        const address = candidate.address ?? ''
        if (!address || candidates.length >= 32) return
        const item = { address, type: candidate.type ?? 'unknown', protocol: candidate.protocol ?? 'unknown', scope: addressScope(address) }
        if (!candidates.some((existing) => existing.address === item.address && existing.type === item.type && existing.protocol === item.protocol)) candidates.push(item)
      }
      timer = setTimeout(() => finish(zh ? '候选收集超时，覆盖不完整' : 'Candidate gathering timed out; coverage incomplete', false), 10000)
      void peer.createOffer().then((offer) => {
        if (!ended) return peer?.setLocalDescription(offer)
      }).catch((error: unknown) => finish(error instanceof Error ? error.message : String(error), false))
    } catch (error) {
      finish(error instanceof Error ? error.message : String(error), false)
    }
  })
}

export async function runBrowserChecks(signal: AbortSignal, zh: boolean, expectedIpv4: string, expectedIpv6: string): Promise<BrowserReport> {
  const startedAt = new Date().toISOString()
  const [ipv4, ipv6, webrtc] = await Promise.all([addressProbe('ipv4', signal, zh), addressProbe('ipv6', signal, zh), webRtcProbe(signal, zh)])
  return { startedAt, origin: window.location.origin, expectedIpv4, expectedIpv6, ipv4, ipv6, webrtc, completed: !signal.aborted }
}
