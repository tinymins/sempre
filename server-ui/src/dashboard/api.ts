import { serverRequest } from '../server-api'
export interface Overview {
  totalSubscriptions: number
  totalNodes: number
  todayRequests: number
  topSubscriptions: { id: string; remark: string | null; creator: { id: string; name: string; email: string }; lastAccessAt: string | null }[]
}

export interface SystemSettings {
  allowRegistration: boolean
}

export interface NetworkList { count: number; items: string[] }

export const dashboardApi = {
  overview: () => serverRequest<Overview>('/overview'),
  geoIpCn: () => serverRequest<NetworkList>('/network/geoip/cn'),
  geoSiteCn: () => serverRequest<NetworkList>('/network/geosite/cn'),
}
