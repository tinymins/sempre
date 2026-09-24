import { FileIcon, Upload as UploadIcon, X as XIcon } from "lucide-react";
import {
  type ChangeEvent,
  forwardRef,
  type ReactNode,
  useCallback,
  useRef,
  useState,
} from "react";
import { cn } from "./utils";

/* ─── types ─── */
export interface UploadFile {
  uid: string;
  name: string;
  status?: "uploading" | "done" | "error" | "removed";
  url?: string;
  thumbUrl?: string;
  percent?: number;
  originFileObj?: File;
  response?: unknown;
  error?: unknown;
  type?: string;
  size?: number;
}

export interface UploadChangeInfo {
  file: UploadFile;
  fileList: UploadFile[];
}

export interface UploadProps {
  /** Accept file types */
  accept?: string;
  /** Multiple files */
  multiple?: boolean;
  /** Controlled file list */
  fileList?: UploadFile[];
  /** Default file list */
  defaultFileList?: UploadFile[];
  /** Upload change callback */
  onChange?: (info: UploadChangeInfo) => void;
  /** Custom upload handler — receives file object */
  customRequest?: (options: {
    file: File;
    onSuccess?: (response?: unknown) => void;
    onError?: (error: Error) => void;
    onProgress?: (event: { percent: number }) => void;
  }) => void;
  /** Before upload — return false to block */
  beforeUpload?: (
    file: File,
    fileList: File[],
  ) => boolean | string | Promise<boolean | File | string>;
  /** Remove callback */
  onRemove?: (
    file: UploadFile,
  ) => boolean | undefined | Promise<boolean | undefined>;
  /** Max count */
  maxCount?: number;
  /** Disabled */
  disabled?: boolean;
  /** Show file list */
  showUploadList?: boolean;
  /** Render as dragger */
  listType?: "text" | "picture" | "picture-card";
  /** Children (trigger area) */
  children?: ReactNode;
  className?: string;
  /** Name attribute for input */
  name?: string;
}

let uid = 0;
const genUid = () => `upload-${++uid}-${Date.now()}`;

const toUploadFile = (file: File): UploadFile => ({
  uid: genUid(),
  name: file.name,
  status: "done",
  type: file.type,
  size: file.size,
  originFileObj: file,
});

const BaseUpload = forwardRef<HTMLDivElement, UploadProps>(function BaseUpload(
  {
    accept,
    multiple = false,
    fileList: controlledFileList,
    defaultFileList,
    onChange,
    customRequest,
    beforeUpload,
    onRemove,
    maxCount,
    disabled = false,
    showUploadList = true,
    listType = "text",
    children,
    className,
    name = "file",
  },
  ref,
) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [internalList, setInternalList] = useState<UploadFile[]>(
    defaultFileList ?? [],
  );
  const list = controlledFileList ?? internalList;

  const update = useCallback(
    (file: UploadFile, newList: UploadFile[]) => {
      if (!controlledFileList) setInternalList(newList);
      onChange?.({ file, fileList: newList });
    },
    [controlledFileList, onChange],
  );

  const handleChange = async (e: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files ?? []);
    if (!files.length) return;

    for (const rawFile of files) {
      if (maxCount && list.length >= maxCount) break;

      if (beforeUpload) {
        const result = await beforeUpload(rawFile, files);
        if (result === false) continue;
      }

      const uf = toUploadFile(rawFile);
      uf.status = "uploading";
      const newList = [...list, uf];
      update(uf, newList);

      if (customRequest) {
        customRequest({
          file: rawFile,
          onSuccess: (resp) => {
            uf.status = "done";
            uf.response = resp;
            update(uf, [...newList]);
          },
          onError: (err) => {
            uf.status = "error";
            uf.error = err;
            update(uf, [...newList]);
          },
          onProgress: ({ percent }) => {
            uf.percent = percent;
            update(uf, [...newList]);
          },
        });
      } else {
        uf.status = "done";
        update(uf, newList);
      }
    }
    // reset input so same file can be re-selected
    if (inputRef.current) inputRef.current.value = "";
  };

  const handleRemove = async (file: UploadFile) => {
    if (onRemove) {
      const res = await onRemove(file);
      if (res === false) return;
    }
    const newList = list.filter((f) => f.uid !== file.uid);
    update({ ...file, status: "removed" }, newList);
  };

  const trigger = () => {
    if (!disabled) inputRef.current?.click();
  };

  return (
    <div ref={ref} className={cn("inline-block", className)}>
      <input
        ref={inputRef}
        type="file"
        accept={accept}
        multiple={multiple}
        name={name}
        className="hidden"
        onChange={handleChange}
        disabled={disabled}
      />

      {/* trigger */}
      {listType === "picture-card" ? (
        <div className="flex flex-wrap gap-2">
          {showUploadList &&
            list.map((f) => (
              <div
                key={f.uid}
                className="group relative w-[104px] h-[104px] rounded-lg border border-border-base overflow-hidden flex items-center justify-center bg-surface-base"
              >
                {f.thumbUrl || f.url ? (
                  <img
                    src={f.thumbUrl || f.url}
                    alt={f.name}
                    className="w-full h-full object-cover"
                  />
                ) : (
                  <FileIcon className="w-6 h-6 text-fg-muted" />
                )}
                <button
                  type="button"
                  className="absolute top-1 right-1 rounded-full bg-black/50 p-0.5 text-white opacity-0 group-hover:opacity-100 transition-opacity"
                  onClick={(e) => {
                    e.stopPropagation();
                    handleRemove(f);
                  }}
                >
                  <XIcon className="w-3 h-3" />
                </button>
              </div>
            ))}
          {(!maxCount || list.length < maxCount) && (
            <button
              type="button"
              onClick={trigger}
              disabled={disabled}
              className={cn(
                "w-[104px] h-[104px] rounded-lg border-2 border-dashed border-border-base flex flex-col items-center justify-center gap-1 text-fg-muted hover:border-sky-400 hover:text-sky-500 transition-colors",
                disabled && "opacity-50 cursor-not-allowed",
              )}
            >
              <UploadIcon className="w-6 h-6" />
              <span className="text-xs">上传</span>
            </button>
          )}
        </div>
      ) : (
        <div
          role="button"
          tabIndex={0}
          onClick={trigger}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") trigger();
          }}
          className={cn(disabled && "opacity-50 cursor-not-allowed")}
        >
          {children ?? (
            <button
              type="button"
              disabled={disabled}
              className="inline-flex items-center gap-1.5 px-4 py-1.5 text-sm border border-border-base rounded-md hover:border-sky-400 transition"
            >
              <UploadIcon className="w-4 h-4" /> 上传文件
            </button>
          )}
        </div>
      )}

      {/* file list (text/picture) */}
      {showUploadList && listType !== "picture-card" && list.length > 0 && (
        <ul className="mt-2 space-y-1">
          {list.map((f) => (
            <li
              key={f.uid}
              className={cn(
                "group flex items-center gap-2 text-sm px-1 py-0.5 rounded hover:bg-fill-tertiary transition-colors",
                f.status === "error" && "text-red-500",
              )}
            >
              {listType === "picture" && (f.thumbUrl || f.url) ? (
                <img
                  src={f.thumbUrl || f.url}
                  alt={f.name}
                  className="w-8 h-8 object-cover rounded"
                />
              ) : (
                <FileIcon className="w-4 h-4 text-fg-muted shrink-0" />
              )}
              <span className="flex-1 min-w-0 truncate text-fg-secondary">
                {f.name}
              </span>
              {f.status === "uploading" && (
                <span className="text-xs text-sky-500 shrink-0">
                  {f.percent != null ? `${Math.round(f.percent)}%` : "上传中…"}
                </span>
              )}
              <button
                type="button"
                onClick={() => handleRemove(f)}
                className="opacity-0 group-hover:opacity-100 transition-opacity text-fg-muted hover:text-red-500"
              >
                <XIcon className="w-3.5 h-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
});

/* ─── Dragger variant ─── */
export interface DraggerProps extends Omit<UploadProps, "children"> {
  children?: ReactNode;
}

export function Dragger(props: DraggerProps) {
  const { children, className, disabled, ...rest } = props;
  const inputRef = useRef<HTMLInputElement>(null);
  const [dragging, setDragging] = useState(false);

  return (
    <BaseUpload {...rest} disabled={disabled} className={className}>
      <div
        role="presentation"
        onDragOver={(e) => {
          e.preventDefault();
          if (!disabled) setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragging(false);
          if (disabled) return;
          const files = e.dataTransfer.files;
          if (files.length) {
            // Create synthetic input change
            const dt = new DataTransfer();
            for (const f of Array.from(files)) dt.items.add(f);
            if (inputRef.current) {
              inputRef.current.files = dt.files;
              inputRef.current.dispatchEvent(
                new Event("change", { bubbles: true }),
              );
            }
          }
        }}
        className={cn(
          "w-full py-8 px-4 flex flex-col items-center gap-2 border-2 border-dashed rounded-lg cursor-pointer transition-colors select-none",
          dragging
            ? "border-sky-400 bg-sky-50 dark:bg-sky-950/30"
            : "border-border-base hover:border-sky-400",
          disabled && "opacity-50 cursor-not-allowed",
        )}
      >
        {children ?? (
          <>
            <UploadIcon className="w-10 h-10 text-sky-500" />
            <p className="text-sm text-fg-muted">点击或拖拽文件到此区域上传</p>
          </>
        )}
      </div>
    </BaseUpload>
  );
}

export const Upload = Object.assign(BaseUpload, {
  Dragger,
  LIST_IGNORE: "__LIST_IGNORE__" as const,
});
