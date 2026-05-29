import { useEffect, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import {
  ingestMaterial,
  listSupportedFiles,
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

export function MaterialFolderPicker({ meetingId, onAllReady }: Props) {
  const [folder, setFolder] = useState<string | null>(null);
  const [files, setFiles] = useState<FileItem[]>([]);
  const [scanning, setScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let unmounted = false;
    onMaterialProgress((evt: MaterialProgressEvent) => {
      setFiles((prev) =>
        prev.map((f) => {
          if (f.path !== evt.file_path) return f;
          if (evt.status === 'started') return { ...f, status: 'indexing' };
          if (evt.status === 'completed') return { ...f, status: 'done' };
          if (evt.status === 'failed') return { ...f, status: 'failed', error: evt.error };
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

  const handleSelectFolder = async () => {
    setScanError(null);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: '选择会议资料文件夹',
      });
      if (!selected || typeof selected !== 'string') return;
      setFolder(selected);
      setScanning(true);

      const paths = await listSupportedFiles(selected);
      const items: FileItem[] = paths.map((p) => ({
        path: p,
        name: p.split('/').pop() || p,
        status: 'pending',
      }));
      setFiles(items);
      setScanning(false);

      if (items.length === 0) {
        setScanError('文件夹里没有支持的文件(.pdf / .docx / .md / .txt)');
        return;
      }

      for (const item of items) {
        try {
          await ingestMaterial(meetingId, item.path);
        } catch (e) {
          setFiles((prev) =>
            prev.map((f) =>
              f.path === item.path ? { ...f, status: 'failed', error: String(e) } : f
            )
          );
        }
      }
    } catch (e) {
      setScanError(String(e));
      setScanning(false);
    }
  };

  const allDone = files.length === 0 || files.every((f) => f.status === 'done' || f.status === 'failed');
  const anyIngested = files.some((f) => f.status === 'done');

  useEffect(() => {
    if (allDone) onAllReady(anyIngested);
  }, [allDone, anyIngested, onAllReady]);

  return (
    <div className="space-y-3">
      <button
        onClick={handleSelectFolder}
        disabled={scanning}
        className="w-full px-4 py-3 border-2 border-dashed border-gray-300 rounded hover:border-blue-400 hover:bg-blue-50 transition disabled:opacity-50 text-left"
      >
        <div className="text-gray-700 font-medium">
          {folder ? `📁 ${folder.split('/').pop() || folder}` : '📁 选择会议资料文件夹'}
        </div>
        <div className="text-xs text-gray-400 mt-1">
          支持 PDF / Word / Markdown / 文本 — 选好后自动索引文件夹及其子文件夹内所有支持文件
        </div>
      </button>

      {scanError && (
        <div className="text-sm text-red-600 px-3 py-2 bg-red-50 border border-red-200 rounded">
          {scanError}
        </div>
      )}

      {files.length > 0 && (
        <ul className="space-y-1 text-sm font-mono border rounded p-3 bg-gray-50 max-h-64 overflow-y-auto">
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
              {f.status === 'failed' && (
                <span className="ml-auto text-xs text-red-600 truncate" title={f.error}>
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
