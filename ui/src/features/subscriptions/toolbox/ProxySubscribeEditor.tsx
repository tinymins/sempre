import { Alert, Checkbox, Form, Input } from '@acme/components'
import { EditorProvider, SubscriptionConfigEditor, type EditorDraft, type EditorSource } from '@acme/subscription-editor'
import { forwardRef, useImperativeHandle, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useI18n } from '@/lib/i18n'
import { type Props, type SaveFeedback, profileFormValues, recommendedEditorDefaults } from './ProxySubscribeModel'
import { useProxySubscribeEditor } from './useProxySubscribeEditor'
import { usePrivateAccessOptions } from './PrivateAccessEditor'
import { ProxyRuntimeFields } from './ProxyRuntimeFields'
import { dnsOptions } from './editor-options'
import SourceDebugModal from './SourceDebugModal'

export type { ProxySubscribeSaveState } from './ProxySubscribeModel'
export interface ProxySubscribeEditorRef { saveNow: () => void }

const ProxySubscribeEditor = forwardRef<ProxySubscribeEditorRef, Props>((props, ref) => {
  const { locale } = useI18n()
  const state = useProxySubscribeEditor(props)
  const { t, form, queueAutosave, saveNow, configurationContext, defaults, features, networkInventory } = state
  const [draft, setDraft] = useState(() => profileFormValues(props.profile) as EditorDraft)
  const [debugSource, setDebugSource] = useState<Extract<EditorSource, { type: 'url' }> | null>(null)
  const privateAccessOptions = usePrivateAccessOptions(props.profile.id)
  const recommended = recommendedEditorDefaults(defaults, configurationContext)
  useImperativeHandle(ref, () => ({ saveNow }), [saveNow])
  const update = (patch: Partial<EditorDraft>) => {
    if (props.readOnly) return
    setDraft(current => ({ ...current, ...patch }))
    form.setFieldsValue(patch)
    queueAutosave()
  }
  const schedule = <div className="grid gap-4 border-t border-[var(--border)] pt-4 md:grid-cols-2">
    <label className="grid gap-1.5 text-sm"><span>{t('proxy.form.updateSchedule')}</span><Input value={state.scheduleInterval} onChange={event => { state.setScheduleInterval(event.target.value); state.queueScheduleSave({ interval: event.target.value }) }} /></label>
    {props.showAutoRestart !== false ? <label className="flex min-h-9 items-center gap-2 self-end rounded-md border border-[var(--border)] px-3 text-sm"><Checkbox checked={state.autoRestart} onChange={event => { state.setAutoRestart(event.target.checked); state.queueScheduleSave({ auto_restart: event.target.checked }, true) }} /><span>{t('proxy.form.restartAfterScheduledUpdates')}</span></label> : null}
  </div>
  const runtime = <>
    <ProxyRuntimeFields supportsLocalProxy={state.supportsLocalProxy} supportsTransparent={state.supportsTransparent} supportsManagement={state.supportsManagement} features={features} form={form} transparentMode={state.transparentMode} tunInterfaceMode={state.tunInterfaceMode} networkInventory={networkInventory} />
  </>
  return <EditorProvider locale={locale}><div className="min-h-0 rounded-lg border border-[var(--border)] p-4">
    <SaveStatus profile={state.profileFeedback} schedule={state.scheduleFeedback} />
    {configurationContext.target && configurationContext.running && configurationContext.target.core !== configurationContext.running.core ? <Alert type="warning" showIcon message={t('proxy.form.coreTransition', { target: configurationContext.target.core, running: configurationContext.running.core })} /> : null}
    <Form form={form} layout="vertical" autoComplete="off" onValuesChange={props.readOnly ? undefined : queueAutosave}><fieldset disabled={props.readOnly} className="m-0 min-w-0 border-0 p-0">
      <SubscriptionConfigEditor value={draft} onChange={update} readOnly={props.readOnly}
        defaults={{ ruleList: recommended.rule_list, group: recommended.group, filter: recommended.filter, customConfig: recommended.custom_config, dnsConfig: recommended.dns_config }}
        features={configurationContext.capabilities.features} protocolCount={configurationContext.capabilities.protocols.length}
        nodes={props.customNodes.map(node => ({ id: node.id, name: node.name, label: `${node.name} · ${String(node.proxy.type || '')} · ${String(node.proxy.server || '')}:${String(node.proxy.port || '')}` }))}
        basicExtension={schedule} dnsOptions={dnsOptions(configurationContext, networkInventory)} privateAccessOptions={privateAccessOptions}
        onDebugSource={state.sourceDebug ? source => { if (source.type === 'url') setDebugSource(source) } : undefined}
        extraTabs={[...(state.runtimeVisible ? [{ key: 'runtime', label: t('proxy.tabs.runtime'), children: runtime }] : []), { key: 'diagnostics', label: t('proxy.tabs.diagnostics'), children: props.diagnostics }]}
      />
    </fieldset></Form>
    {debugSource ? <SourceDebugModal open item={debugSource} onClose={() => setDebugSource(null)} /> : null}
  </div></EditorProvider>
})
ProxySubscribeEditor.displayName = 'ProxySubscribeEditor'

function SaveStatus({ profile, schedule }: { profile: SaveFeedback; schedule: SaveFeedback }) {
  const { t } = useTranslation();
  const feedback = profile.state === "error" ? profile
    : schedule.state === "error" ? schedule
      : profile.state === "saving" || schedule.state === "saving" ? { state: "saving" as const }
        : profile.state === "waiting" || schedule.state === "waiting" ? { state: "waiting" as const }
          : profile.state === "saved" || schedule.state === "saved" ? { state: "saved" as const }
            : { state: "idle" as const };
  const label = feedback.state === "waiting" ? t("proxy.autosave.waiting")
    : feedback.state === "saving" ? t("proxy.autosave.saving")
      : feedback.state === "saved" ? t("proxy.autosave.saved")
        : feedback.message || "";
  return (
    <div className="mb-4 min-h-6 border-b border-gray-200 pb-3 text-sm dark:border-gray-700">
      {label ? <p role={feedback.state === "error" ? "alert" : "status"} className={feedback.state === "error" ? "break-words text-red-500" : "text-[var(--text-secondary)]"}>{label}</p> : null}
    </div>
  );
}


export default ProxySubscribeEditor
