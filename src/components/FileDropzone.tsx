import { useCallback, useEffect, useRef, useState } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import {
  ingestMaterial,
  onMaterialProgress,
  MaterialProgressEvent,
} from '../lib/tauri-bridge';

interface FileItem {
  path: string;
  name: string;
  status: 'pending' | 'indexing' | 'done' | 'failed';
  error?: string;
}

interface Props {
  meetingId: string;
  onAllReady: (anyIngested: boolean) => void;
}

const ACCEPTED_EXTS = new Set(['.pdf', '.docx', '.md', '.txt']);

function getExt(p: string): string {
  const i = p.lastIndexOf('.');
  return i >= 0 ? p.slice(i).toLowerCase() : '';
}

function basename(p: string): string {
  return p.split(/[\\/]/).pop() ?? p;
}

export function FileDropzone({ meetingId, onAllReady }: Props) {
  const [files, setFiles] = useState<FileItem[]>([]);
  const [isDragOver, setIsDragOver] = useState(false);
  // Track paths already enqueued to avoid double-ingesting on re-drop
  const enqueuedPaths = useRef<Set<string>>(new Set());

  // Listen to material_progress events
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let unmounted = false;
    onMaterialProgress((evt: MaterialProgressEvent) => {
      setFiles((prev) =>
        prev.map((f) => {
          if (f.path !== evt.file_path) return f;
          if (evt.status === 'started') return { ...f, status: 'indexing' };
          if (evt.status === 'completed') return { ...f, status: 'done' };
          if (evt.status === 'failed')
            return { ...f, status: 'failed', error: evt.error };
          return f;
        })
      );
    }).then((fn) => {
      if (unmounted) fn();
      else unlisten = fn;
    });
    return () => {
      unmounted = true;
      unlisten?.();
    };
  }, []);

  // Tauri native drag-drop: provides real filesystem paths
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let unmounted = false;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === 'enter' || event.payload.type === 'over') {
          setIsDragOver(true);
        } else if (event.payload.type === 'leave') {
          setIsDragOver(false);
        } else if (event.payload.type === 'drop') {
          setIsDragOver(false);
          const paths = event.payload.paths.filter((p) =>
            ACCEPTED_EXTS.has(getExt(p))
          );
          handlePaths(paths);
        }
      })
      .then((fn) => {
        if (unmounted) fn();
        else unlisten = fn;
      });
    return () => {
      unmounted = true;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meetingId]);

  const handlePaths = useCallback(
    async (paths: string[]) => {
      const newPaths = paths.filter((p) => !enqueuedPaths.current.has(p));
      if (newPaths.length === 0) return;

      newPaths.forEach((p) => enqueuedPaths.current.add(p));

      const newItems: FileItem[] = newPaths.map((p) => ({
        path: p,
        name: basename(p),
        status: 'pending',
      }));
      setFiles((prev) => [...prev, ...newItems]);

      // Ingest serially to avoid hammering the embedding API
      for (const item of newItems) {
        try {
          await ingestMaterial(meetingId, item.path);
        } catch (e) {
          // material_progress event will also set failed status,
          // but set here as fallback in case event doesn't fire
          setFiles((prev) =>
            prev.map((f) =>
              f.path === item.path
                ? { ...f, status: 'failed', error: String(e) }
                : f
            )
          );
        }
      }
    },
    [meetingId]
  );

  const allDone =
    files.length === 0 ||
    files.every((f) => f.status === 'done' || f.status === 'failed');
  const anyIngested = files.some((f) => f.status === 'done');

  // Notify parent when all files reach terminal state
  useEffect(() => {
    if (allDone) onAllReady(anyIngested);
  }, [allDone, anyIngested, onAllReady]);

  return (
    <div className="space-y-3">
      <div
        className={`p-6 border-2 border-dashed rounded text-center transition select-none ${
          isDragOver
            ? 'border-blue-500 bg-blue-50'
            : 'border-gray-300 hover:border-gray-400'
        }`}
      >
        <p className="text-gray-600">
          拖拽 PDF / Word / Markdown / 文本到这里
        </p>
        <p className="text-xs text-gray-400 mt-1">
          支持 .pdf .docx .md .txt — 会议前的资料(报价单、客户档案、规范、底线表)
        </p>
      </div>

      {files.length > 0 && (
        <ul className="space-y-1 text-sm font-mono border rounded p-3 bg-gray-50">
          {files.map((f) => (
            <li
              key={f.path}
              className={`flex items-center gap-2 ${
                f.status === 'done'
                  ? 'text-green-700'
                  : f.status === 'failed'
                  ? 'text-red-700'
                  : 'text-gray-700'
              }`}
            >
              <span className="shrink-0 w-5">
                {f.status === 'pending' && '⏳'}
                {f.status === 'indexing' && '🔄'}
                {f.status === 'done' && '✓'}
                {f.status === 'failed' && '✗'}
              </span>
              <span className="truncate">{f.name}</span>
              {f.status === 'indexing' && (
                <span className="ml-auto text-xs text-gray-500">索引中...</span>
              )}
              {f.status === 'failed' && (
                <span
                  className="ml-auto text-xs text-red-600 truncate"
                  title={f.error}
                >
                  {f.error}
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
