import {
  AutoComplete,
  Button,
  Checkbox,
  DeleteOutlined,
  HolderOutlined,
  Input,
  InputNumber,
  PlayCircleOutlined,
  PlusOutlined,
  Select,
  Tooltip,
} from "@acme/components";
import type { ProxySourceFetchMode, SubscribeItem } from "@acme/types";
import {
  closestCenter,
  DndContext,
  type DragEndEvent,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import SourceDebugModal from "./SourceDebugModal";

interface Props {
  value?: SubscribeItem[];
  onChange?: (items: SubscribeItem[]) => void;
  allowDebug?: boolean;
}

const UA_PRESETS = [
  {
    value: "clash.meta",
    label: "Clash Meta (default)",
  },
  {
    value: "ClashforWindows/0.20.39",
    label: "Clash for Windows",
  },
  {
    value: "clash-verge/v2.2.3",
    label: "Clash Verge",
  },
  {
    value: "stash/2.7.6",
    label: "Stash",
  },
  {
    value: "Quantumult%20X/1.4.1",
    label: "Quantumult X",
  },
  {
    value: "Shadowrocket/2050",
    label: "Shadowrocket",
  },
  {
    value: "v2rayN/7.6",
    label: "v2rayN",
  },
];

const emptyItem = (): SubscribeItem => ({
  id: crypto.randomUUID(),
  enabled: true,
  url: "",
  prefix: "",
  remark: "",
  cacheTtlMinutes: undefined,
  fetchUa: undefined,
  fetchMode: "auto",
});

/** 单个订阅源卡片（可排序） */
const SortableCard = ({
  item,
  sortId,
  onUpdate,
  onRemove,
  allowDebug,
}: {
  item: SubscribeItem;
  sortId: string;
  onUpdate: (patch: Partial<SubscribeItem>) => void;
  onRemove: () => void;
  allowDebug: boolean;
}) => {
  const { t } = useTranslation();
  const [showTestModal, setShowTestModal] = useState(false);
  const fetchModeOptions = [
    {
      value: "auto",
      label: t("proxy.form.fetchModeAuto"),
    },
    {
      value: "domestic-direct",
      label: t("proxy.form.fetchModeDomesticDirect"),
    },
  ];

  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: sortId });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
  };

  return (
    <>
      <div
        ref={setNodeRef}
        style={style}
        className={`rounded-lg border p-3 transition-colors ${
          item.enabled
            ? "border-gray-200 bg-white dark:border-gray-600 dark:bg-[#1a1a1a]"
            : "border-dashed border-gray-300 bg-gray-50 opacity-60 dark:border-gray-700 dark:bg-[#111]"
        }`}
      >
        <div className="flex gap-2">
          {/* Left gutter: drag handle + checkbox */}
          <div className="flex items-center gap-2 shrink-0 self-start pt-[5px]">
            <span
              {...attributes}
              {...listeners}
              className="cursor-grab text-gray-400 hover:text-gray-600 dark:hover:text-gray-300"
            >
              <HolderOutlined />
            </span>
            <Tooltip
              title={
                item.enabled
                  ? t("proxy.form.subscribeItemEnabled")
                  : t("proxy.form.subscribeItemDisabled")
              }
            >
              <Checkbox
                checked={item.enabled}
                onChange={(e) => onUpdate({ enabled: e.target.checked })}
              />
            </Tooltip>
          </div>

          {/* Main content — URL/delete and details naturally left/right-align */}
          <div className="flex-1 min-w-0 space-y-2">
            {/* Row 1: URL + delete */}
            <div className="flex items-center gap-2">
              <Input
                size="small"
                placeholder={t("proxy.form.subscribeItemUrl")}
                value={item.url}
                onChange={(e) => onUpdate({ url: e.target.value })}
                status={
                  item.enabled && !item.url?.trim() ? "warning" : undefined
                }
                className="flex-1 min-w-0"
              />
              <Button
                variant="text"
                size="small"
                danger
                aria-label={t("proxy.actions.delete")}
                icon={<DeleteOutlined />}
                onClick={onRemove}
                className="shrink-0"
              />
            </div>

            {/* Row 2: remark + prefix + cache | UA + test (own line on mobile) */}
            <div className="flex flex-col gap-2 md:flex-row md:items-center">
              <Input
                size="small"
                placeholder={t("proxy.form.subscribeItemRemark")}
                value={item.remark}
                onChange={(e) => onUpdate({ remark: e.target.value })}
                className="md:flex-1 min-w-0"
              />
              <div className="flex items-center gap-2 shrink-0">
                <div className="flex-1 md:flex-none md:w-[120px]">
                  <Input
                    size="small"
                    placeholder={t("proxy.form.subscribeItemPrefix")}
                    value={item.prefix}
                    onChange={(e) => onUpdate({ prefix: e.target.value })}
                    suffix={
                      <Tooltip title={t("proxy.form.subscribeItemPrefixTip")}>
                        <span className="text-gray-400 text-xs cursor-help">
                          ?
                        </span>
                      </Tooltip>
                    }
                  />
                </div>
                <Tooltip title={t("proxy.form.cacheTtlTooltip")}>
                  <InputNumber
                    size="small"
                    min={0}
                    max={1440}
                    placeholder={t(
                      "proxy.form.subscribeItemCacheTtlPlaceholder",
                    )}
                    value={item.cacheTtlMinutes}
                    onChange={(val) =>
                      onUpdate({ cacheTtlMinutes: val ?? undefined })
                    }
                    addonAfter={t("proxy.form.cacheTtlUnit")}
                    style={{ width: 130 }}
                  />
                </Tooltip>
              </div>
              {/* UA + test: own line on mobile, inline on PC */}
              <div className="flex items-center gap-2 md:flex-1">
                <Tooltip title={t("proxy.form.fetchModeTooltip")}>
                  <div className="w-[120px] shrink-0">
                    <Select
                      size="small"
                      value={item.fetchMode ?? "auto"}
                      options={fetchModeOptions}
                      onChange={(value) =>
                        onUpdate({
                          fetchMode: value as ProxySourceFetchMode,
                        })
                      }
                      className="w-full"
                    />
                  </div>
                </Tooltip>
                <Tooltip title={t("proxy.form.fetchUaTooltip")}>
                  <div className="flex-1">
                    <AutoComplete
                      size="small"
                      options={UA_PRESETS}
                      value={item.fetchUa ?? ""}
                      onChange={(v) => onUpdate({ fetchUa: v || undefined })}
                      placeholder={t("proxy.form.fetchUaPlaceholder")}
                      filterOption={(input, opt) =>
                        opt.value.toLowerCase().includes(input.toLowerCase()) ||
                        (typeof opt.label === "string" &&
                          opt.label.toLowerCase().includes(input.toLowerCase()))
                      }
                      allowClear
                      className="w-full"
                    />
                  </div>
                </Tooltip>
                {allowDebug ? <Tooltip title={t("proxy.form.testSourceBtn")}>
                  <Button
                    size="small"
                    variant="text"
                    icon={<PlayCircleOutlined />}
                    onClick={() => setShowTestModal(true)}
                    disabled={!item.url?.trim()}
                    className="cursor-pointer shrink-0"
                  />
                </Tooltip> : null}
              </div>
            </div>
          </div>
        </div>
      </div>

      {allowDebug ? <SourceDebugModal
        open={showTestModal}
        item={item}
        onClose={() => setShowTestModal(false)}
      /> : null}
    </>
  );
};

const SubscribeItemsEditor = ({ value = [], onChange, allowDebug = true }: Props) => {
  const { t } = useTranslation();
  const items = useMemo(() => value, [value]);

  const [idCounter, setIdCounter] = useState(() => items.length);
  const [sortIds, setSortIds] = useState<string[]>(() =>
    items.map((item, i) => item.id ?? `item-${i}`),
  );

  const effectiveSortIds = useMemo(() => {
    if (sortIds.length === items.length) return sortIds;
    if (sortIds.length < items.length) {
      const newIds = [...sortIds];
      for (let i = sortIds.length; i < items.length; i++) {
        newIds.push(`item-${idCounter + i - sortIds.length}`);
      }
      return newIds;
    }
    return sortIds.slice(0, items.length);
  }, [sortIds, items.length, idCounter]);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
  );

  const update = useCallback(
    (index: number, patch: Partial<SubscribeItem>) => {
      const next = items.map((item, i) =>
        i === index ? { ...item, ...patch } : item,
      );
      onChange?.(next);
    },
    [items, onChange],
  );

  const add = useCallback(() => {
    const newId = `item-${idCounter}`;
    setIdCounter((c) => c + 1);
    setSortIds([...effectiveSortIds, newId]);
    onChange?.([...items, emptyItem()]);
  }, [items, onChange, idCounter, effectiveSortIds]);

  const remove = useCallback(
    (index: number) => {
      const next = items.filter((_, i) => i !== index);
      const nextIds = effectiveSortIds.filter((_, i) => i !== index);
      setSortIds(nextIds);
      onChange?.(next);
    },
    [items, onChange, effectiveSortIds],
  );

  const handleDragEnd = useCallback(
    (event: DragEndEvent) => {
      const { active, over } = event;
      if (!over || active.id === over.id) return;

      const oldIndex = effectiveSortIds.indexOf(active.id as string);
      const newIndex = effectiveSortIds.indexOf(over.id as string);
      if (oldIndex === -1 || newIndex === -1) return;

      const newItems = arrayMove(items, oldIndex, newIndex);
      const newIds = arrayMove(effectiveSortIds, oldIndex, newIndex);
      setSortIds(newIds);
      onChange?.(newItems);
    },
    [items, onChange, effectiveSortIds],
  );

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      onDragEnd={handleDragEnd}
    >
      <SortableContext
        items={effectiveSortIds}
        strategy={verticalListSortingStrategy}
      >
        <div className="space-y-3">
          {items.map((item, index) => (
            <SortableCard
              key={effectiveSortIds[index]}
              sortId={effectiveSortIds[index]}
              item={item}
              onUpdate={(patch) => update(index, patch)}
              onRemove={() => remove(index)}
              allowDebug={allowDebug}
            />
          ))}
        </div>
      </SortableContext>

      <Button
        variant="dashed"
        block
        icon={<PlusOutlined />}
        onClick={add}
        className="!mt-3"
      >
        {t("proxy.form.addSubscribeItem")}
      </Button>
    </DndContext>
  );
};

export default SubscribeItemsEditor;
