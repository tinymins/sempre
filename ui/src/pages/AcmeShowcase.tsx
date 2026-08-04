import { useState, type ReactNode } from 'react'
import {
  Alert,
  AppSidebar,
  AutoComplete,
  Avatar,
  Badge,
  Button,
  Card,
  Checkbox,
  CloudServerOutlined,
  Collapse,
  ContextMenu,
  ControlOutlined,
  DeleteOutlined,
  Descriptions,
  Divider,
  Dragger,
  DragHandle,
  Drawer,
  Dropdown,
  EmojiPicker,
  Empty,
  FileTextOutlined,
  FolderOutlined,
  Form,
  GlobalOutlined,
  HorizontalScroll,
  Image,
  InlineEmojiPicker,
  Input,
  InputNumber,
  List,
  Menu,
  Modal,
  Pagination,
  PathBar,
  PillTabBar,
  PlusOutlined,
  Popconfirm,
  Popover,
  PosterCard,
  Progress,
  SaveOutlined,
  ScrollNav,
  SearchInput,
  SegmentedToggle,
  Select,
  SettingsMenu,
  Skeleton,
  Slider,
  Spin,
  Statistic,
  Switch,
  Table,
  Tabs,
  Tag,
  TemplateInput,
  TextArea,
  Tooltip,
  Upload,
  UserOutlined,
  useForm,
  useToast,
  type TableColumnsType,
} from '@acme/components'
import { AcmeContentBoundary } from '../components/AcmeContentBoundary'

interface DemoRow {
  id: string
  name: string
  type: string
  latency: number
}

const demoRows: DemoRow[] = [
  { id: 'hk-01', name: 'Hong Kong 01', type: 'VLESS', latency: 42 },
  { id: 'sg-02', name: 'Singapore 02', type: 'Trojan', latency: 86 },
  { id: 'jp-03', name: 'Tokyo 03', type: 'Shadowsocks', latency: 64 },
]

const tableColumns: TableColumnsType<DemoRow> = [
  { title: 'Name', dataIndex: 'name', key: 'name' },
  { title: 'Protocol', dataIndex: 'type', key: 'type', render: (value) => <Tag color="blue">{String(value)}</Tag> },
  { title: 'Latency', dataIndex: 'latency', key: 'latency', align: 'right', render: (value) => `${String(value)} ms` },
]

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="border-b border-[var(--border-base)] py-8 first:pt-0 last:border-b-0">
      <h2 className="mb-5 text-base font-semibold text-[var(--text-primary)]">{title}</h2>
      {children}
    </section>
  )
}

function Sample({ name, children, className = '' }: { name: string; children: ReactNode; className?: string }) {
  return (
    <div className={`min-w-0 ${className}`}>
      <div className="mb-3 text-xs font-medium text-[var(--text-muted)]">{name}</div>
      {children}
    </div>
  )
}

function ShowcaseContent() {
  const toast = useToast()
  const [form] = useForm<Record<string, unknown>>({ name: 'Local subscription', port: 7890, enabled: true })
  const [checked, setChecked] = useState(true)
  const [switched, setSwitched] = useState(true)
  const [segment, setSegment] = useState(true)
  const [slider, setSlider] = useState(62)
  const [number, setNumber] = useState<number | null>(7890)
  const [page, setPage] = useState(2)
  const [path, setPath] = useState('/subscriptions/local/rules')
  const [pill, setPill] = useState<'all' | 'active'>('all')
  const [sort, setSort] = useState('name')
  const [emoji, setEmoji] = useState('⚡')
  const [template, setTemplate] = useState('{{name}}-{{index}}')
  const [modalOpen, setModalOpen] = useState(false)
  const [drawerOpen, setDrawerOpen] = useState(false)
  const [sidebarKey, setSidebarKey] = useState('subscriptions')

  return (
    <div>
      <div className="mb-2 flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold">ACME Components</h1>
          <p className="mt-1 text-sm text-[var(--text-muted)]">toolbox source snapshot 969c0db</p>
        </div>
        <Tag color="green">Development only</Tag>
      </div>

      <Section title="Buttons, status and icons">
        <div className="grid gap-6 xl:grid-cols-2">
          <Sample name="Button">
            <div className="flex flex-wrap items-center gap-2">
              <Button variant="primary" icon={<PlusOutlined />}>Primary</Button>
              <Button icon={<SaveOutlined />}>Default</Button>
              <Button variant="dashed">Dashed</Button>
              <Button variant="text">Text</Button>
              <Button variant="link">Link</Button>
              <Button variant="danger" icon={<DeleteOutlined />}>Danger</Button>
              <Button loading>Loading</Button>
              <Button shape="circle" icon={<PlusOutlined />} aria-label="Add" />
            </div>
          </Sample>
          <Sample name="Badge, Tag and Avatar">
            <div className="flex flex-wrap items-center gap-4">
              <Badge count={8}><Avatar icon={<UserOutlined />} /></Badge>
              <Badge status="success" text="Online" />
              <Badge status="processing" text="Updating" />
              <Tag color="blue">VLESS</Tag>
              <Tag color="green" closable>Healthy</Tag>
              <Tag color="#0d9488" icon={<GlobalOutlined />}>Custom</Tag>
              <Avatar size={40}>SP</Avatar>
              <Avatar shape="square" icon={<CloudServerOutlined />} />
            </div>
          </Sample>
          <Sample name="Icon aliases" className="xl:col-span-2">
            <div className="flex flex-wrap gap-5 text-xl text-[var(--text-secondary)]">
              <PlusOutlined /><SaveOutlined /><DeleteOutlined /><UserOutlined /><ControlOutlined />
              <CloudServerOutlined /><GlobalOutlined /><FolderOutlined /><FileTextOutlined />
            </div>
          </Sample>
        </div>
      </Section>

      <Section title="Inputs and selection">
        <div className="grid gap-6 md:grid-cols-2 xl:grid-cols-3">
          <Sample name="Input">
            <div className="space-y-3">
              <Input placeholder="Subscription name" />
              <Input.Password placeholder="Password" />
              <SearchInput placeholder="Search nodes" />
              <TextArea rows={3} placeholder="Notes" />
            </div>
          </Sample>
          <Sample name="Select and AutoComplete">
            <div className="space-y-3">
              <Select defaultValue="vless" options={[{ value: 'vless', label: 'VLESS' }, { value: 'trojan', label: 'Trojan' }, { value: 'ss', label: 'Shadowsocks' }]} />
              <Select mode="multiple" defaultValue={['hk', 'jp']} options={[{ value: 'hk', label: 'Hong Kong' }, { value: 'jp', label: 'Japan' }, { value: 'sg', label: 'Singapore' }]} />
              <AutoComplete allowClear placeholder="User agent" options={['clash.meta', 'sing-box', 'v2rayN']} />
            </div>
          </Sample>
          <Sample name="Numeric and binary controls">
            <div className="space-y-5">
              <InputNumber value={number} min={1} max={65535} addonAfter="port" onChange={setNumber} />
              <Slider value={slider} onChange={setSlider} className="w-full" />
              <div className="flex flex-wrap items-center gap-5">
                <Checkbox checked={checked} onChange={(event) => setChecked(event.target.checked)}>Enabled</Checkbox>
                <Switch checked={switched} onChange={setSwitched} />
              </div>
              <SegmentedToggle value={segment} onChange={setSegment} checkedLabel="Direct" uncheckedLabel="Proxy" />
            </div>
          </Sample>
          <Sample name="Checkbox.Group">
            <Checkbox.Group defaultValue={['dns', 'rules']} options={[{ label: 'DNS', value: 'dns' }, { label: 'Rules', value: 'rules' }, { label: 'Nodes', value: 'nodes' }]} />
          </Sample>
          <Sample name="TemplateInput">
            <TemplateInput value={template} onChange={(event) => setTemplate(event.target.value)} vars={[{ key: 'name', label: 'Node name' }, { key: 'index', label: 'Node index' }, { key: 'type', label: 'Protocol' }]} />
          </Sample>
          <Sample name="EmojiPicker">
            <EmojiPicker value={emoji} onChange={setEmoji} onClear={() => setEmoji('')} clearLabel="Clear" />
          </Sample>
        </div>
      </Section>

      <Section title="Form">
        <div className="max-w-2xl">
          <Form form={form} layout="vertical" onFinish={(values) => toast.success(`Saved ${String(values.name)}`)}>
            <div className="grid gap-5 md:grid-cols-2">
              <Form.Item name="name" label="Name" required tooltip="Shown in the subscription list">
                <Input placeholder="Subscription name" />
              </Form.Item>
              <Form.Item name="port" label="Mixed port" required>
                <InputNumber min={1} max={65535} className="w-full" />
              </Form.Item>
            </div>
            <Form.Item name="enabled" valuePropName="checked">
              <Switch checkedChildren="On" unCheckedChildren="Off" />
            </Form.Item>
            <Button htmlType="submit" variant="primary" icon={<SaveOutlined />}>Validate and save</Button>
          </Form>
        </div>
      </Section>

      <Section title="Navigation">
        <div className="grid gap-8 xl:grid-cols-2">
          <Sample name="Tabs">
            <div className="space-y-5">
              <Tabs defaultActiveKey="source" items={[{ key: 'source', label: 'Source', children: <div className="py-4 text-sm">Source content</div> }, { key: 'transform', label: 'Transform', children: <div className="py-4 text-sm">Transform content</div> }, { key: 'output', label: 'Output', disabled: true }]} />
              <Tabs type="segment" defaultActiveKey="one" items={[{ key: 'one', label: 'General' }, { key: 'two', label: 'Advanced' }]} />
              <Tabs type="pill" defaultActiveKey="all" items={[{ key: 'all', label: 'All' }, { key: 'ready', label: 'Ready' }]} />
            </div>
          </Sample>
          <Sample name="PillTabBar">
            <PillTabBar
              sticky={false}
              tabs={[{ key: 'all', label: 'All', icon: GlobalOutlined }, { key: 'active', label: 'Active', icon: CloudServerOutlined }]}
              activeTab={pill}
              onTabChange={setPill}
              sort={{ options: [{ label: 'Name', value: 'name' }, { label: 'Latency', value: 'latency' }], value: sort, onChange: setSort }}
              trailing={<Badge count={3} />}
            />
          </Sample>
          <Sample name="PathBar">
            <PathBar path={path} onNavigate={setPath} rootLabel={<FolderOutlined />} />
          </Sample>
          <Sample name="Pagination">
            <Pagination current={page} total={128} pageSize={10} showSizeChanger showTotal={(total, range) => `${range[0]}-${range[1]} / ${total}`} onChange={setPage} />
          </Sample>
          <Sample name="Menu">
            <div className="max-w-sm rounded-lg border border-[var(--border-base)] p-2">
              <Menu defaultSelectedKeys={['subscriptions']} defaultOpenKeys={['network']} items={[{ key: 'network', label: 'Network', icon: <GlobalOutlined />, children: [{ key: 'subscriptions', label: 'Subscriptions' }, { key: 'proxies', label: 'Proxies' }] }, { key: 'settings', label: 'Settings', icon: <ControlOutlined /> }]} />
            </div>
          </Sample>
          <Sample name="AppSidebar">
            <div className="h-72 overflow-hidden rounded-lg border border-[var(--border-base)]">
              <AppSidebar
                width={240}
                header={<span className="font-semibold">Toolbox</span>}
                sections={[{ label: 'Workspace', items: [{ key: 'subscriptions', label: 'Subscriptions', subtitle: '3 sources', icon: <GlobalOutlined /> }, { key: 'rules', label: 'Rules', subtitle: '128 entries', icon: <FileTextOutlined /> }] }]}
                activeKey={sidebarKey}
                onSelect={setSidebarKey}
                footer={<span className="text-xs text-[var(--text-muted)]">Local workspace</span>}
              />
            </div>
          </Sample>
          <Sample name="ScrollNav" className="xl:col-span-2">
            <ScrollNav className="h-64" items={[{ key: 'general', label: 'General', icon: ControlOutlined }, { key: 'network', label: 'Network', icon: GlobalOutlined }]}>
              <ScrollNav.Section id="general" title="General" className="min-h-52">
                <Descriptions column={1} items={[{ label: 'Name', children: 'Sempre' }, { label: 'Mode', children: 'Local' }]} />
              </ScrollNav.Section>
              <ScrollNav.Section id="network" title="Network" className="min-h-52">
                <Descriptions column={1} items={[{ label: 'Port', children: '7890' }, { label: 'DNS', children: 'Enabled' }]} />
              </ScrollNav.Section>
            </ScrollNav>
          </Sample>
          <Sample name="SettingsMenu" className="xl:col-span-2">
            <div className="h-64 max-w-xl overflow-hidden rounded-lg border border-[var(--border-base)] p-3">
              <SettingsMenu rootLabel="Settings" items={[{ key: 'general', label: 'General', desc: 'Theme and language', icon: <ControlOutlined />, content: <div className="space-y-3"><Input defaultValue="Sempre" /><Select defaultValue="system" options={[{ value: 'system', label: 'System' }, { value: 'dark', label: 'Dark' }]} /></div> }, { key: 'network', label: 'Network', desc: 'Ports and connectivity', icon: <GlobalOutlined />, items: [{ key: 'proxy', label: 'Proxy', desc: 'Mixed port', content: <InputNumber defaultValue={7890} /> }] }]} />
            </div>
          </Sample>
        </div>
      </Section>

      <Section title="Data display">
        <div className="grid gap-8 xl:grid-cols-2">
          <Sample name="Table" className="xl:col-span-2">
            <Table<DemoRow> rowKey="id" bordered columns={tableColumns} dataSource={demoRows} pagination={false} rowSelection={{ selectedRowKeys: ['hk-01'] }} />
          </Sample>
          <Sample name="List">
            <List dataSource={demoRows} rowKey="id" bordered header="Nodes" renderItem={(item) => <List.Item extra={<Tag color="green">{item.latency} ms</Tag>}><List.Item.Meta avatar={<Avatar>{item.name.slice(0, 2)}</Avatar>} title={item.name} description={item.type} /></List.Item>} />
          </Sample>
          <Sample name="Descriptions">
            <Descriptions bordered column={2} title="Runtime" items={[{ label: 'Core', children: 'Mihomo' }, { label: 'Status', children: <Badge status="success" text="Running" /> }, { label: 'Version', children: '1.19.9' }, { label: 'Mode', children: 'Rule' }]} />
          </Sample>
          <Sample name="Card">
            <Card title="Subscription" extra={<Button size="small" variant="text">Edit</Button>} actions={[<span key="sync">Sync</span>, <span key="details">Details</span>]}>
              <p className="text-sm text-[var(--text-secondary)]">Local source · 36 nodes</p>
            </Card>
          </Sample>
          <Sample name="PosterCard">
            <div className="w-40">
              <PosterCard fallback={<div className="grid h-full place-items-center text-4xl text-[var(--text-muted)]"><FolderOutlined /></div>} badges={<Tag color="blue" className="absolute right-2 top-2">Local</Tag>}><span className="block truncate text-sm font-medium">Subscription asset</span></PosterCard>
            </div>
          </Sample>
          <Sample name="Image">
            <Image src="/__acme_showcase_missing__.png" alt="Fallback state" width={180} height={112} preview={false} />
          </Sample>
          <Sample name="HorizontalScroll">
            <HorizontalScroll className="max-w-md" innerClassName="gap-2 pb-2">
              {Array.from({ length: 10 }, (_, index) => <Tag key={index} color={index % 2 ? 'blue' : 'green'}>Node {index + 1}</Tag>)}
            </HorizontalScroll>
          </Sample>
          <Sample name="Empty">
            <Empty description="No converted nodes" />
          </Sample>
          <Sample name="DragHandle">
            <div className="flex items-center gap-2 rounded-lg border border-[var(--border-base)] p-3 text-sm"><DragHandle />Reorderable row</div>
          </Sample>
          <Sample name="Divider">
            <div className="text-sm"><span>Source</span><Divider>Converted output</Divider><span>Result</span></div>
          </Sample>
        </div>
      </Section>

      <Section title="Feedback">
        <div className="grid gap-8 xl:grid-cols-2">
          <Sample name="Alert">
            <div className="space-y-3">
              <Alert type="success" showIcon message="Conversion completed" description="36 nodes were generated." />
              <Alert type="warning" showIcon closable message="Source is stale" />
              <Alert type="error" showIcon message="Validation failed" />
            </div>
          </Sample>
          <Sample name="Progress and Statistic">
            <div className="space-y-5">
              <Progress percent={72} status="active" />
              <div className="flex items-center gap-8"><Progress type="circle" percent={86} size={72} /><Statistic title="Converted nodes" value={36} suffix="nodes" /></div>
            </div>
          </Sample>
          <Sample name="Skeleton">
            <Skeleton avatar active paragraph={{ rows: 3 }} />
          </Sample>
          <Sample name="Spin">
            <Spin spinning tip="Converting"><div className="h-28 rounded-lg border border-[var(--border-base)] p-4 text-sm">Subscription output</div></Spin>
          </Sample>
          <Sample name="Collapse" className="xl:col-span-2">
            <Collapse defaultActiveKey="one" items={[{ key: 'one', label: 'Conversion summary', children: '36 nodes, 12 rules and 4 proxy groups.' }, { key: 'two', label: 'Warnings', children: 'No blocking warnings.' }]} />
          </Sample>
        </div>
      </Section>

      <Section title="Floating and overlay">
        <div className="flex flex-wrap items-center gap-3">
          <Tooltip title="Refresh source"><Button icon={<GlobalOutlined />}>Tooltip</Button></Tooltip>
          <Popover trigger="click" title="Source details" content="Updated 2 minutes ago"><Button>Popover</Button></Popover>
          <Dropdown trigger={['click']} menu={{ items: [{ key: 'edit', label: 'Edit', icon: <ControlOutlined /> }, { key: 'delete', label: 'Delete', icon: <DeleteOutlined />, danger: true }] }}><Button>Dropdown</Button></Dropdown>
          <Popconfirm title="Delete this source?" description="This action only affects the local copy." okType="danger"><Button variant="danger">Popconfirm</Button></Popconfirm>
          <ContextMenu items={[{ key: 'edit', label: 'Edit' }, { key: 'duplicate', label: 'Duplicate' }, { key: 'delete', label: 'Delete', danger: true }]}><div className="rounded-lg border border-dashed border-[var(--border-base)] px-4 py-2 text-sm">ContextMenu</div></ContextMenu>
          <Button onClick={() => setModalOpen(true)}>Modal</Button>
          <Button onClick={() => setDrawerOpen(true)}>Drawer</Button>
          <Button variant="primary" onClick={() => toast.success('Subscription saved')}>Toast</Button>
        </div>
        <Modal open={modalOpen} title="Edit subscription" onCancel={() => setModalOpen(false)} onOk={() => { setModalOpen(false); return undefined }}>
          <Form layout="vertical"><Form.Item label="Name"><Input defaultValue="Local subscription" /></Form.Item></Form>
        </Modal>
        <Drawer open={drawerOpen} title="Subscription details" onClose={() => setDrawerOpen(false)} footer={<Button block variant="primary" onClick={() => setDrawerOpen(false)}>Done</Button>}>
          <Descriptions column={1} items={[{ label: 'Nodes', children: '36' }, { label: 'Rules', children: '128' }]} />
        </Drawer>
      </Section>

      <Section title="Upload and emoji">
        <div className="grid gap-8 xl:grid-cols-2">
          <Sample name="Upload">
            <Upload beforeUpload={() => false}><Button icon={<PlusOutlined />}>Choose file</Button></Upload>
          </Sample>
          <Sample name="Dragger">
            <Dragger beforeUpload={() => false} />
          </Sample>
          <Sample name="InlineEmojiPicker" className="overflow-hidden xl:col-span-2">
            <InlineEmojiPicker onSelect={setEmoji} />
          </Sample>
        </div>
      </Section>
    </div>
  )
}

export function AcmeShowcase() {
  return <AcmeContentBoundary><ShowcaseContent /></AcmeContentBoundary>
}
