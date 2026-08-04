/**
 * @acme/components - 通用 UI 组件库
 *
 * 此包包含与业务逻辑无关的通用 UI 组件，可在多个应用间复用。
 */

export type { AlertProps } from "./Alert";
export { Alert } from "./Alert";
export type {
  AppSidebarItem,
  AppSidebarProps,
  AppSidebarSection,
} from "./AppSidebar";
export { AppSidebar } from "./AppSidebar";
export type { AutoCompleteOption, AutoCompleteProps } from "./AutoComplete";
export { AutoComplete } from "./AutoComplete";
export type { AvatarProps } from "./Avatar";
export { Avatar } from "./Avatar";
export type { BadgeProps } from "./Badge";
export { Badge } from "./Badge";
export type { ButtonProps } from "./Button";
// ─── Base components ───
export { Button } from "./Button";
export type { CardProps } from "./Card";
export { Card } from "./Card";
export type { CheckboxProps } from "./Checkbox";
export { Checkbox } from "./Checkbox";
export type { CollapseItem, CollapseProps } from "./Collapse";
export { Collapse } from "./Collapse";
export type {
  ActiveDescendantControls,
  ContextMenuItem,
  ContextMenuProps,
} from "./ContextMenu";
export {
  ActiveDescendantContext,
  ContextMenu,
  useContextMenu,
} from "./ContextMenu";
export type { DescriptionsItem, DescriptionsProps } from "./Descriptions";
export { Descriptions } from "./Descriptions";
export type { DividerProps } from "./Divider";
export { Divider } from "./Divider";
export type { DrawerProps } from "./Drawer";
export { Drawer } from "./Drawer";
export type {
  DropdownMenuConfig,
  DropdownMenuItem,
  DropdownProps,
} from "./Dropdown";
export { Dropdown } from "./Dropdown";
export type { DragHandleProps, UseDndOptions, UseDndReturn } from "./dnd";
export { DragHandle, useDnd } from "./dnd";
export type { EmojiPickerProps } from "./EmojiPicker";
export { EmojiPicker } from "./EmojiPicker";
export type { EmptyProps } from "./Empty";
export { Empty } from "./Empty";
export type { FieldValues as FormFieldValues, FormInstance } from "./Form";
export {
  Form,
  FormItemTooltip,
  useForm,
  useFormContext,
  useWatch,
} from "./Form";
export type { HorizontalScrollProps } from "./HorizontalScroll";
export { HorizontalScroll } from "./HorizontalScroll";
export type { ImageProps } from "./Image";
export { Image } from "./Image";
export type { InlineEmojiPickerProps } from "./InlineEmojiPicker";
export { InlineEmojiPicker } from "./InlineEmojiPicker";
export { Input, Password, SearchInput, TextArea } from "./Input";
export type { InputNumberProps } from "./InputNumber";
export { InputNumber } from "./InputNumber";
// ─── Icons (re-exported from lucide with antd-compatible names) ───
export * from "./icons";
export type { ListProps } from "./List";
export { List } from "./List";
export type { MenuItem as MenuItemType, MenuProps } from "./Menu";
export { Menu } from "./Menu";
export type { ConfirmConfig, ModalProps, ScaledModalSize } from "./Modal";
// ─── Feedback / Navigation ───
export {
  Modal,
  ModalContainerContext,
  setActiveModalContainer,
  useConfirm,
} from "./Modal";
export type { PaginationProps } from "./Pagination";
export { Pagination } from "./Pagination";
export type { PathBarProps } from "./PathBar";
export { PathBar } from "./PathBar";
export type {
  PillTab,
  PillTabBarProps,
  PillTabFilter,
  PillTabSort,
  PillTabSortOption,
} from "./PillTabBar";
export { PillTabBar } from "./PillTabBar";
export type { PopconfirmProps } from "./Popconfirm";
export { Popconfirm } from "./Popconfirm";
export type { PopoverProps } from "./Popover";
export { Popover } from "./Popover";
export type { PosterCardProps } from "./PosterCard";
export { PosterCard } from "./PosterCard";
export type { ProgressProps } from "./Progress";
export { Progress } from "./Progress";
export type {
  ScrollNavItem,
  ScrollNavProps,
  ScrollNavSectionProps,
} from "./ScrollNav";
export { ScrollNav } from "./ScrollNav";
export type { SegmentedToggleProps } from "./SegmentedToggle";
export { SegmentedToggle } from "./SegmentedToggle";
export type { SelectOption, SelectProps } from "./Select";
// ─── Complex components ───
export { Select } from "./Select";
export type { SettingsMenuItem, SettingsMenuProps } from "./SettingsMenu";
export { SettingsMenu } from "./SettingsMenu";
export type { SkeletonProps } from "./Skeleton";
export { Skeleton } from "./Skeleton";
export type { SliderProps } from "./Slider";
export { Slider } from "./Slider";
export type { SpinProps } from "./Spin";
export { Spin } from "./Spin";
export type { StatisticProps } from "./Statistic";
export { Statistic } from "./Statistic";
export type { SwitchProps } from "./Switch";
export { Switch } from "./Switch";
export type { TableColumn, TableColumnsType, TableProps } from "./Table";
export { Table } from "./Table";
export type { TabItem, TabsProps } from "./Tabs";
export { Tabs } from "./Tabs";
export type { TagProps } from "./Tag";
export { Tag } from "./Tag";
export type { TemplateInputProps, TemplateVariable } from "./TemplateInput";
export { TemplateInput } from "./TemplateInput";
export type { ToastType } from "./Toast";
export { ToastProvider, useToast } from "./Toast";
export type { TooltipProps } from "./Tooltip";
// ─── Overlay / Floating ───
export { Tooltip } from "./Tooltip";
// ─── Context ───
export type { UIContextValue } from "./UIContext";
export { UIContext, useUIContext } from "./UIContext";
export type { UploadChangeInfo, UploadFile, UploadProps } from "./Upload";
export { Dragger, Upload } from "./Upload";
// ─── Utilities ───
export { cn } from "./utils";
