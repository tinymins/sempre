import { serverRequest } from '../server-api'
import type { SystemSettings } from './api'

export interface AdminUser {
  id: string
  name: string
  email: string
  role: 'superadmin' | 'admin' | 'user'
  lastLoginAt: string | null
  createdAt: string
}

export interface Invitation {
  id: string
  code: string
  createdBy: string
  usedBy: string | null
  usedAt: string | null
  expiresAt: string | null
  createdAt: string
}

export const adminApi = {
  settings: () => serverRequest<SystemSettings>('/admin/settings'),
  updateSettings: (body: { allowRegistration?: boolean }) => serverRequest<SystemSettings>('/admin/settings', { method: 'PATCH', body: JSON.stringify(body) }),
  users: () => serverRequest<AdminUser[]>('/admin/users'),
  createUser: (body: { name: string; email: string; password: string; role: AdminUser['role'] }) => serverRequest<AdminUser>('/admin/users', { method: 'POST', body: JSON.stringify(body) }),
  updateRole: (id: string, role: AdminUser['role']) => serverRequest<AdminUser>(`/admin/users/${encodeURIComponent(id)}`, { method: 'PATCH', body: JSON.stringify({ role }) }),
  resetPassword: (id: string, newPassword: string) => serverRequest<void>(`/admin/users/${encodeURIComponent(id)}/password`, { method: 'POST', body: JSON.stringify({ newPassword }) }),
  deleteUser: (id: string) => serverRequest<void>(`/admin/users/${encodeURIComponent(id)}`, { method: 'DELETE' }),
  invitations: () => serverRequest<Invitation[]>('/admin/invitations'),
  createInvitation: (expiresInHours?: number) => serverRequest<Invitation>('/admin/invitations', { method: 'POST', body: JSON.stringify(expiresInHours ? { expiresInHours } : {}) }),
  deleteInvitation: (id: string) => serverRequest<void>(`/admin/invitations/${encodeURIComponent(id)}`, { method: 'DELETE' }),
}
