/**
 * Icon mapping layer: re-exports lucide-react icons with antd-like names.
 * All icons default to size="1em" so they inherit the container's font-size,
 * matching antd icon behavior. Explicit size/className props can still override.
 *
 * Usage:
 *   import { DeleteOutlined, EditOutlined } from "@acme/components/icons";
 *   // or
 *   import { DeleteOutlined } from "@acme/components";
 */

import {
  AlertCircle,
  AlertTriangle,
  ArrowDown,
  ArrowLeft,
  ArrowLeftRight,
  ArrowUp,
  ArrowUpRight,
  BarChart3,
  Bell,
  Bug,
  Calendar,
  Check,
  CheckCircle,
  ChevronDown,
  ChevronRight,
  Clock,
  CloudDownload,
  CloudUpload,
  Copy,
  Crosshair,
  Database,
  Download,
  ExternalLink,
  Eye,
  EyeOff,
  File,
  FileText,
  Filter,
  Folder,
  FolderOpen,
  Globe,
  GripVertical,
  HardDrive,
  HelpCircle,
  History,
  Hourglass,
  Import,
  Info,
  LayoutDashboard,
  Lightbulb,
  ListMusic,
  ListTodo,
  Loader2,
  Lock,
  LogOut,
  Menu,
  MinusCircle,
  Moon,
  Music,
  Palette,
  PauseCircle,
  Pencil,
  Play,
  PlayCircle,
  Plug,
  Plus,
  Redo,
  RefreshCcw,
  RefreshCw,
  Rocket,
  Save,
  ScanLine,
  Search,
  Send,
  Server,
  Settings,
  Shield,
  ShieldCheck,
  Square,
  Star,
  Sun,
  Timer,
  Trash2,
  Upload,
  User,
  UserPlus,
  Video,
  X,
  XCircle,
  Zap,
} from "lucide-react";
import {
  type ComponentProps,
  type ForwardRefExoticComponent,
  forwardRef,
  type RefAttributes,
} from "react";

/* ── Helper: wrap a lucide icon to default size="1em" ── */
type LucideIcon = ForwardRefExoticComponent<
  ComponentProps<typeof Star> & RefAttributes<SVGSVGElement>
>;

function withDefaultSize(Icon: LucideIcon, displayName: string): LucideIcon {
  const Wrapped = forwardRef<SVGSVGElement, ComponentProps<typeof Star>>(
    function WrappedIcon({ size = "1em", ...props }, ref) {
      return <Icon ref={ref} size={size} {...props} />;
    },
  );
  Wrapped.displayName = displayName;
  return Wrapped as unknown as LucideIcon;
}

/* ─── Status / Indicator Icons ─── */
export const ExclamationCircleOutlined = withDefaultSize(
  AlertCircle,
  "ExclamationCircleOutlined",
);
export const WarningOutlined = withDefaultSize(
  AlertTriangle,
  "WarningOutlined",
);
export const CheckCircleOutlined = withDefaultSize(
  CheckCircle,
  "CheckCircleOutlined",
);
export const ClockCircleOutlined = withDefaultSize(
  Clock,
  "ClockCircleOutlined",
);
export const QuestionCircleOutlined = withDefaultSize(
  HelpCircle,
  "QuestionCircleOutlined",
);
export const InfoCircleOutlined = withDefaultSize(Info, "InfoCircleOutlined");
export const MinusCircleOutlined = withDefaultSize(
  MinusCircle,
  "MinusCircleOutlined",
);
export const PauseCircleOutlined = withDefaultSize(
  PauseCircle,
  "PauseCircleOutlined",
);
export const PlayCircleOutlined = withDefaultSize(
  PlayCircle,
  "PlayCircleOutlined",
);
export const CloseCircleOutlined = withDefaultSize(
  XCircle,
  "CloseCircleOutlined",
);
export const HourglassOutlined = withDefaultSize(
  Hourglass,
  "HourglassOutlined",
);

/* ─── Navigation / UI Icons ─── */
export const ArrowDownOutlined = withDefaultSize(
  ArrowDown,
  "ArrowDownOutlined",
);
export const ArrowLeftOutlined = withDefaultSize(
  ArrowLeft,
  "ArrowLeftOutlined",
);
export const ArrowUpOutlined = withDefaultSize(ArrowUp, "ArrowUpOutlined");
export const SwapOutlined = withDefaultSize(ArrowLeftRight, "SwapOutlined");
export const DownOutlined = withDefaultSize(ChevronDown, "DownOutlined");
export const RightOutlined = withDefaultSize(ChevronRight, "RightOutlined");
export const MenuOutlined = withDefaultSize(Menu, "MenuOutlined");
export const HolderOutlined = withDefaultSize(GripVertical, "HolderOutlined");
export const CloseOutlined = withDefaultSize(X, "CloseOutlined");
export const SearchOutlined = withDefaultSize(Search, "SearchOutlined");

/* ─── Media / Domain Icons ─── */
export const CalendarOutlined = withDefaultSize(Calendar, "CalendarOutlined");
export const CloudDownloadOutlined = withDefaultSize(
  CloudDownload,
  "CloudDownloadOutlined",
);
export const CloudUploadOutlined = withDefaultSize(
  CloudUpload,
  "CloudUploadOutlined",
);
export const DatabaseOutlined = withDefaultSize(Database, "DatabaseOutlined");
export const DownloadOutlined = withDefaultSize(Download, "DownloadOutlined");
export const FileOutlined = withDefaultSize(File, "FileOutlined");
export const FileTextOutlined = withDefaultSize(FileText, "FileTextOutlined");
export const FolderOutlined = withDefaultSize(Folder, "FolderOutlined");
export const FolderOpenOutlined = withDefaultSize(
  FolderOpen,
  "FolderOpenOutlined",
);
export const GlobalOutlined = withDefaultSize(Globe, "GlobalOutlined");
export const HddOutlined = withDefaultSize(HardDrive, "HddOutlined");
export const HistoryOutlined = withDefaultSize(History, "HistoryOutlined");
export const PlaySquareOutlined = withDefaultSize(Play, "PlaySquareOutlined");
export const VideoCameraOutlined = withDefaultSize(
  Video,
  "VideoCameraOutlined",
);
export const CloudServerOutlined = withDefaultSize(
  Server,
  "CloudServerOutlined",
);
export const StarOutlined = withDefaultSize(Star, "StarOutlined");
export const DashboardOutlined = withDefaultSize(
  LayoutDashboard,
  "DashboardOutlined",
);
export const BarChartOutlined = withDefaultSize(BarChart3, "BarChartOutlined");
export const ListTodoOutlined = withDefaultSize(ListTodo, "ListTodoOutlined");
export const ListMusicOutlined = withDefaultSize(
  ListMusic,
  "ListMusicOutlined",
);
export const PaletteOutlined = withDefaultSize(Palette, "PaletteOutlined");

/* ─── Operation Icons ─── */
export const BellOutlined = withDefaultSize(Bell, "BellOutlined");
export const CheckOutlined = withDefaultSize(Check, "CheckOutlined");
export const CopyOutlined = withDefaultSize(Copy, "CopyOutlined");
export const DeleteOutlined = withDefaultSize(Trash2, "DeleteOutlined");
export const EditOutlined = withDefaultSize(Pencil, "EditOutlined");
export const EyeOutlined = withDefaultSize(Eye, "EyeOutlined");
export const EyeInvisibleOutlined = withDefaultSize(
  EyeOff,
  "EyeInvisibleOutlined",
);
export const FilterOutlined = withDefaultSize(Filter, "FilterOutlined");
export const ImportOutlined = withDefaultSize(Import, "ImportOutlined");
export const LinkOutlined = withDefaultSize(ExternalLink, "LinkOutlined");
export const BulbOutlined = withDefaultSize(Lightbulb, "BulbOutlined");
export const LockOutlined = withDefaultSize(Lock, "LockOutlined");
export const LogoutOutlined = withDefaultSize(LogOut, "LogoutOutlined");
export const MoonOutlined = withDefaultSize(Moon, "MoonOutlined");
export const MusicOutlined = withDefaultSize(Music, "MusicOutlined");
export const SunOutlined = withDefaultSize(Sun, "SunOutlined");
export const ApiOutlined = withDefaultSize(Plug, "ApiOutlined");
export const PlusOutlined = withDefaultSize(Plus, "PlusOutlined");
export const RedoOutlined = withDefaultSize(Redo, "RedoOutlined");
export const RocketOutlined = withDefaultSize(Rocket, "RocketOutlined");
export const SaveOutlined = withDefaultSize(Save, "SaveOutlined");
export const ScanOutlined = withDefaultSize(ScanLine, "ScanOutlined");
export const SendOutlined = withDefaultSize(Send, "SendOutlined");
export const ControlOutlined = withDefaultSize(Settings, "ControlOutlined");
export const ShieldOutlined = withDefaultSize(Shield, "ShieldOutlined");
export const ShieldCheckOutlined = withDefaultSize(
  ShieldCheck,
  "ShieldCheckOutlined",
);
export const StopOutlined = withDefaultSize(Square, "StopOutlined");
export const FieldTimeOutlined = withDefaultSize(Timer, "FieldTimeOutlined");
export const ThunderboltOutlined = withDefaultSize(Zap, "ThunderboltOutlined");
export const UploadOutlined = withDefaultSize(Upload, "UploadOutlined");
export const UserOutlined = withDefaultSize(User, "UserOutlined");
export const UserAddOutlined = withDefaultSize(UserPlus, "UserAddOutlined");
export const BugOutlined = withDefaultSize(Bug, "BugOutlined");
export const ExportOutlined = withDefaultSize(ArrowUpRight, "ExportOutlined");
export const AimOutlined = withDefaultSize(Crosshair, "AimOutlined");

/* ─── Special icons (custom wrappers) ─── */

const StarFilledBase = Star;

export const StarFilled = forwardRef<
  SVGSVGElement,
  ComponentProps<typeof Star>
>(function StarFilled({ size = "1em", ...props }, ref) {
  return (
    <StarFilledBase ref={ref} size={size} {...props} fill="currentColor" />
  );
});

export const LoadingOutlined = forwardRef<
  SVGSVGElement,
  ComponentProps<typeof Loader2> & { spin?: boolean }
>(function LoadingOutlined({ spin, className, size = "1em", ...props }, ref) {
  const cls =
    [spin !== false ? "animate-spin" : "", className ?? ""]
      .filter(Boolean)
      .join(" ") || undefined;
  return <Loader2 ref={ref} className={cls} size={size} {...props} />;
});

export const SyncOutlined = forwardRef<
  SVGSVGElement,
  ComponentProps<typeof RefreshCcw> & { spin?: boolean }
>(function SyncOutlined({ spin, className, size = "1em", ...props }, ref) {
  const cls =
    [spin ? "animate-spin" : "", className ?? ""].filter(Boolean).join(" ") ||
    undefined;
  return <RefreshCcw ref={ref} className={cls} size={size} {...props} />;
});

export const ReloadOutlined = forwardRef<
  SVGSVGElement,
  ComponentProps<typeof RefreshCw> & { spin?: boolean }
>(function ReloadOutlined({ spin, className, size = "1em", ...props }, ref) {
  const cls =
    [spin ? "animate-spin" : "", className ?? ""].filter(Boolean).join(" ") ||
    undefined;
  return <RefreshCw ref={ref} className={cls} size={size} {...props} />;
});
