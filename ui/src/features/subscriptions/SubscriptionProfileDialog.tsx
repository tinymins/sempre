import { type FormEvent } from 'react'
import { Modal, Select } from '@acme/components'
import { Field, Input } from '../../components/ui'
import { useI18n } from '../../lib/i18n'

export type SubscriptionMode = 'local' | 'remote'

export function SubscriptionProfileDialog({
  open,
  creating,
  name,
  mode,
  manifestURL,
  error,
  pending,
  onNameChange,
  onModeChange,
  onManifestURLChange,
  onCancel,
  onSubmit,
  afterOpenChange,
}: {
  open: boolean
  creating: boolean
  name: string
  mode: SubscriptionMode
  manifestURL: string
  error: string
  pending: boolean
  onNameChange: (value: string) => void
  onModeChange: (value: SubscriptionMode) => void
  onManifestURLChange: (value: string) => void
  onCancel: () => void
  onSubmit: () => void
  afterOpenChange: (open: boolean) => void
}) {
  const { t } = useI18n()
  const title = creating ? t('newSubscriptionSet') : t('renameSubscriptionSet')
  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (!pending) onSubmit()
  }
  const invalid = !name.trim() || (creating && mode === 'remote' && !manifestURL.trim())
  return (
    <Modal
      open={open}
      title={title}
      okText={creating ? t('createSubscriptionSet') : t('renameSubscriptionSet')}
      cancelText={t('cancel')}
      onOk={() => { onSubmit(); return undefined }}
      onCancel={onCancel}
      afterOpenChange={afterOpenChange}
      okButtonProps={{ disabled: invalid }}
      cancelButtonProps={{ disabled: pending }}
      confirmLoading={pending}
      maskClosable={!pending}
      keyboard={!pending}
      closable={!pending}
      destroyOnClose
      centered
    >
      <form className="space-y-4" onSubmit={submit}>
        <Field label={t('subscriptionSetName')}>
          <Input autoFocus aria-invalid={Boolean(error)} aria-describedby={error ? 'subscription-set-name-error' : undefined} value={name} onChange={(event) => onNameChange(event.target.value)} />
        </Field>
        {creating ? (
          <Field label={t('subscriptionMode')}>
            <Select
              value={mode}
              options={[
                { value: 'local', label: t('localSubscriptionMode') },
                { value: 'remote', label: t('remoteSubscriptionMode') },
              ]}
              onChange={(value) => onModeChange(value as SubscriptionMode)}
            />
          </Field>
        ) : null}
        {creating && mode === 'remote' ? (
          <Field label={t('remoteManifestURL')} hint={t('remoteManifestHint')}>
            <Input aria-label={t('remoteManifestURL')} type="url" value={manifestURL} onChange={(event) => onManifestURLChange(event.target.value)} />
          </Field>
        ) : null}
        {error ? <p id="subscription-set-name-error" className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}
      </form>
    </Modal>
  )
}
