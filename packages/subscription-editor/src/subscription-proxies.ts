import { createContext } from 'react'
export interface SubscriptionProxyOption { id: string; name: string; enabled: boolean; status: string }
export const SubscriptionProxyContext = createContext<SubscriptionProxyOption[] | undefined>(undefined)
