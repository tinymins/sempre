import { useEffect, useState } from 'react'
import { Select } from '@acme/components'
import { EditorProvider, PrivateAccessEditor as SharedPrivateAccessEditor, type PrivateAccessEditorProps } from '@acme/subscription-editor'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { useI18n } from '@/lib/i18n'
import { api } from '@/lib/api'
import { useOptionalSession } from '@/lib/session'
import type { ManagedRuntimeStatus, Session, TunnelStatus } from '@/lib/types'
import { PrivateAccessHomeNetwork } from './PrivateAccessHomeNetwork'
import { FieldLabel } from './PrivateAccessConfig'

export default function PrivateAccessEditor({ value, onChange, profileId, variant }: { value?: string; onChange?: (value: string) => void; profileId: string; variant?: 'default' | 'simple' }) {
  const { locale } = useI18n()
  const options = usePrivateAccessOptions(profileId)
  return <EditorProvider locale={locale}><SharedPrivateAccessEditor value={value} onChange={onChange} variant={variant} {...options} /></EditorProvider>
}

export function usePrivateAccessOptions(profileId: string): Pick<PrivateAccessEditorProps, 'renderTransport' | 'renderHomeNetwork'> {
  const session = useOptionalSession()?.session
  const status = useRuntimeStatus(session)
  const runtime = status?.private_access?.profile_id === profileId ? status.private_access : undefined
  return {
    renderTransport: (connector, update) => session ? <TransportTunnelSelect session={session} value={connector.transportEndpointRef} onChange={transportEndpointRef => update({ transportEndpointRef })} /> : null,
    renderHomeNetwork: (connector, index, update) => <PrivateAccessHomeNetwork enabled={connector.homeNetworkEnabled} networkIds={connector.homeNetworkIds} runtime={runtime}
      connectorStatus={runtime?.connectors.find(item => item.tag === (connector.tag.trim() || `private-access-${index + 1}`))} onChange={update} />,
  }
}

function useRuntimeStatus(session?: Session | null) {
  const [status, setStatus] = useState<ManagedRuntimeStatus>();
  useEffect(() => {
    if (!session) return;
    let active = true;
    const load = () => {
      void api<ManagedRuntimeStatus>(session, "/runtime/status")
        .then((value) => { if (active) setStatus(value); })
        .catch(() => { if (active) setStatus(undefined); });
    };
    load();
    const timer = window.setInterval(load, 3000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [session]);
  return status;
}

function TransportTunnelSelect({ session, value, onChange }: { session: NonNullable<ReturnType<typeof useOptionalSession>>['session']; value: string; onChange: (value: string) => void }) {
  const { t } = useTranslation();
  const tunnels = useQuery({ queryKey: ["tunnels"], queryFn: () => api<TunnelStatus>(session!, "/tunnels") });
  const options = (tunnels.data?.forwards ?? []).map((forward) => ({ value: forward.forward_id, label: `${forward.instance_name} / ${forward.forward_name} · ${forward.host}:${forward.port}` }));
  return <label className="space-y-1 md:col-span-3"><FieldLabel>{t("proxy.form.privateTunnelForward")}</FieldLabel><Select allowClear value={value || undefined} options={options} placeholder={t("proxy.form.privateTunnelDirect")} onChange={(forwardID) => onChange(forwardID || "")} className="w-full" /></label>;
}
