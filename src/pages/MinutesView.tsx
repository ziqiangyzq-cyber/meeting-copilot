import { useEffect, useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import {
  generateMinutes,
  onMinutesToken,
  onMinutesComplete,
  onMinutesError,
  exportMinutesDocx,
} from '../lib/tauri-bridge';

interface Props {
  meetingId: string;
  meetingName: string;
  onBack: () => void;
}

export function MinutesView({ meetingId, meetingName, onBack }: Props) {
  const [markdown, setMarkdown] = useState<string>('');
  const [status, setStatus] = useState<'streaming' | 'complete' | 'error'>('streaming');
  const [error, setError] = useState<string | null>(null);
  const [copyConfirm, setCopyConfirm] = useState(false);
  const accumRef = useRef<string>('');
  const dispatched = useRef(false);
  const scrollEndRef = useRef<HTMLDivElement>(null);

  // Subscribe to streaming events
  useEffect(() => {
    let unlistenToken: (() => void) | undefined;
    let unlistenComplete: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;
    let unmounted = false;

    onMinutesToken((token) => {
      accumRef.current += token;
      setMarkdown(accumRef.current);
    }).then((fn) => {
      if (unmounted) fn();
      else unlistenToken = fn;
    });

    onMinutesComplete((finalMd) => {
      // Use the final markdown from backend (in case streaming missed any chunks)
      setMarkdown(finalMd || accumRef.current);
      setStatus('complete');
    }).then((fn) => {
      if (unmounted) fn();
      else unlistenComplete = fn;
    });

    onMinutesError((err) => {
      setError(err);
      setStatus('error');
    }).then((fn) => {
      if (unmounted) fn();
      else unlistenError = fn;
    });

    return () => {
      unmounted = true;
      unlistenToken?.();
      unlistenComplete?.();
      unlistenError?.();
    };
  }, []);

  // Trigger generation once
  useEffect(() => {
    if (dispatched.current) return;
    dispatched.current = true;
    generateMinutes(meetingId).catch((e) => {
      // Error event also fires; this catch is belt-and-suspenders
      console.error('generate_minutes failed', e);
      setError(String(e));
      setStatus('error');
    });
  }, [meetingId]);

  // Auto-scroll to bottom while streaming
  useEffect(() => {
    if (status === 'streaming') {
      scrollEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [markdown, status]);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(markdown);
      setCopyConfirm(true);
      setTimeout(() => setCopyConfirm(false), 1500);
    } catch (e) {
      console.error('copy failed', e);
    }
  };

  const handleSave = async () => {
    const fileName = `${meetingName.replace(/[/\\?*:|"<>]/g, '_')}_纪要.md`;
    try {
      const path = await save({
        defaultPath: fileName,
        filters: [{ name: 'Markdown', extensions: ['md'] }],
      });
      if (!path) return;
      await writeTextFile(path, markdown);
    } catch (e) {
      console.error('save failed', e);
      alert(`保存失败: ${e}`);
    }
  };

  const handleSaveDocx = async () => {
    if (!markdown) return;
    const safeName = (meetingName || 'meeting').replace(/[/\\?*:|"<>]/g, '_');
    const fileName = `${safeName}_纪要.docx`;
    try {
      const path = await save({
        defaultPath: fileName,
        filters: [{ name: 'Word', extensions: ['docx'] }],
      });
      if (!path) return;
      await exportMinutesDocx(markdown, path);
    } catch (e) {
      console.error('save docx failed', e);
      alert(`保存失败: ${e}`);
    }
  };

  return (
    <div className="h-screen flex flex-col bg-white">
      <header className="px-6 py-3 border-b flex items-center gap-3 shrink-0">
        <h1 className="text-lg font-bold">会议纪要</h1>
        <span className="text-xs text-gray-500">{meetingName}</span>
        <div className="flex-1" />
        {status === 'streaming' && (
          <span className="text-sm text-blue-600">
            <span className="inline-block w-1.5 h-1.5 bg-blue-600 rounded-full animate-pulse mr-1.5" />
            生成中...
          </span>
        )}
        {status === 'complete' && (
          <>
            <button
              onClick={handleCopy}
              className="px-3 py-1.5 bg-gray-100 hover:bg-gray-200 text-gray-700 text-sm rounded"
            >
              {copyConfirm ? '✓ 已复制' : '复制 MD'}
            </button>
            <button
              onClick={handleSave}
              className="px-3 py-1.5 bg-gray-100 hover:bg-gray-200 text-gray-700 text-sm rounded"
            >
              保存为 .md
            </button>
            <button
              onClick={handleSaveDocx}
              className="px-3 py-1.5 bg-gray-100 hover:bg-gray-200 text-gray-700 text-sm rounded"
            >
              保存为 .docx
            </button>
          </>
        )}
        <button
          onClick={onBack}
          className="px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white text-sm rounded"
        >
          返回
        </button>
      </header>

      {error && (
        <div className="mx-6 mt-4 p-3 bg-red-50 border border-red-200 text-red-800 rounded text-sm">
          ⚠ 纪要生成失败: {error}
        </div>
      )}

      <div className="flex-1 overflow-y-auto px-6 py-4">
        <article className="max-w-3xl mx-auto prose prose-sm prose-headings:font-bold prose-headings:mt-6 prose-headings:mb-3 prose-h1:text-2xl prose-h2:text-lg prose-h2:border-b prose-h2:pb-1 prose-p:my-2 prose-ul:my-2 prose-li:my-0.5">
          {markdown ? (
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{markdown}</ReactMarkdown>
          ) : status === 'streaming' ? (
            <div className="text-gray-400 italic">等待 LLM 启动...</div>
          ) : null}
          <div ref={scrollEndRef} />
        </article>
      </div>
    </div>
  );
}
