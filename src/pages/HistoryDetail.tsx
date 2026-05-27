import { useEffect, useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import {
  getMeetingDetail,
  MeetingDetail,
  generateMinutes,
  onMinutesToken,
  onMinutesComplete,
  onMinutesError,
} from '../lib/tauri-bridge';

interface Props {
  meetingId: string;
  onBack: () => void;
}

type Tab = 'minutes' | 'transcript' | 'suggestions';

export function HistoryDetail({ meetingId, onBack }: Props) {
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [tab, setTab] = useState<Tab>('minutes');
  const [error, setError] = useState<string | null>(null);
  const [regenerating, setRegenerating] = useState(false);
  const [regenError, setRegenError] = useState<string | null>(null);
  const [streamMd, setStreamMd] = useState<string>('');
  const [copyConfirm, setCopyConfirm] = useState(false);
  const accumRef = useRef<string>('');

  // Load detail
  useEffect(() => {
    let cancelled = false;
    getMeetingDetail(meetingId)
      .then((d) => {
        if (!cancelled) setDetail(d);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [meetingId]);

  // Subscribe to minutes streaming (for regenerate)
  useEffect(() => {
    let unlistenToken: (() => void) | undefined;
    let unlistenComplete: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;
    let unmounted = false;

    onMinutesToken((token) => {
      accumRef.current += token;
      setStreamMd(accumRef.current);
    }).then((fn) => {
      if (unmounted) fn();
      else unlistenToken = fn;
    });

    onMinutesComplete((md) => {
      const finalMd = md || accumRef.current;
      setDetail((prev) =>
        prev
          ? {
              ...prev,
              latest_minutes_md: finalMd,
              latest_minutes_version: (prev.latest_minutes_version || 0) + 1,
            }
          : prev
      );
      setStreamMd('');
      accumRef.current = '';
      setRegenerating(false);
    }).then((fn) => {
      if (unmounted) fn();
      else unlistenComplete = fn;
    });

    onMinutesError((err) => {
      setRegenError(err);
      setStreamMd('');
      accumRef.current = '';
      setRegenerating(false);
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

  const handleRegenerate = async () => {
    setRegenError(null);
    setRegenerating(true);
    accumRef.current = '';
    setStreamMd('');
    try {
      await generateMinutes(meetingId);
    } catch (e) {
      setRegenError(String(e));
      setRegenerating(false);
    }
  };

  const handleCopy = async () => {
    if (!detail?.latest_minutes_md) return;
    try {
      await navigator.clipboard.writeText(detail.latest_minutes_md);
      setCopyConfirm(true);
      setTimeout(() => setCopyConfirm(false), 1500);
    } catch (e) {
      console.error('copy failed', e);
    }
  };

  const handleSave = async () => {
    if (!detail?.latest_minutes_md) return;
    const safeName = (detail.meeting.name || 'meeting').replace(/[/\\?*:|"<>]/g, '_');
    const fileName = `${safeName}_纪要.md`;
    try {
      const path = await save({
        defaultPath: fileName,
        filters: [{ name: 'Markdown', extensions: ['md'] }],
      });
      if (!path) return;
      await writeTextFile(path, detail.latest_minutes_md);
    } catch (e) {
      console.error('save failed', e);
      alert(`保存失败: ${e}`);
    }
  };

  const fmtTime = (ms: number) => {
    const d = new Date(ms);
    return `${d.getHours().toString().padStart(2, '0')}:${d
      .getMinutes()
      .toString()
      .padStart(2, '0')}:${d.getSeconds().toString().padStart(2, '0')}`;
  };

  if (error) {
    return (
      <div className="min-h-screen bg-white p-8 max-w-3xl mx-auto">
        <button onClick={onBack} className="text-sm text-blue-600 mb-4">
          ← 返回列表
        </button>
        <div className="p-3 bg-red-50 border border-red-200 text-red-800 rounded text-sm">
          ⚠ {error}
        </div>
      </div>
    );
  }

  if (!detail) {
    return (
      <div className="min-h-screen bg-white p-8 max-w-3xl mx-auto text-gray-400">加载中...</div>
    );
  }

  const displayMd = streamMd || detail.latest_minutes_md;

  return (
    <div className="h-screen flex flex-col bg-white">
      <header className="px-6 py-3 border-b shrink-0">
        <div className="flex items-center gap-3">
          <button onClick={onBack} className="text-sm text-blue-600 hover:underline">
            ← 返回列表
          </button>
          <h1 className="text-lg font-bold">{detail.meeting.name}</h1>
          <div className="flex-1" />
          {tab === 'minutes' && detail.latest_minutes_md && !regenerating && (
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
            </>
          )}
          {tab === 'minutes' && (
            <button
              onClick={handleRegenerate}
              disabled={regenerating}
              className="px-3 py-1.5 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white text-sm rounded"
            >
              {regenerating ? '生成中...' : '重新生成纪要'}
            </button>
          )}
        </div>
        <div className="text-xs text-gray-500 mt-1 flex gap-3 flex-wrap">
          {detail.meeting.project_ref && <span>项目: {detail.meeting.project_ref}</span>}
          {detail.meeting.purpose && <span>{detail.meeting.purpose}</span>}
          <span>
            {detail.transcripts.length} 条转写 · {detail.suggestions.length} 条建议
          </span>
          {detail.latest_minutes_version && <span>纪要 v{detail.latest_minutes_version}</span>}
        </div>
        <div className="mt-3 flex gap-4 text-sm">
          <TabBtn active={tab === 'minutes'} onClick={() => setTab('minutes')}>
            📝 纪要
          </TabBtn>
          <TabBtn active={tab === 'transcript'} onClick={() => setTab('transcript')}>
            📜 转写 ({detail.transcripts.length})
          </TabBtn>
          <TabBtn active={tab === 'suggestions'} onClick={() => setTab('suggestions')}>
            💡 建议 ({detail.suggestions.length})
          </TabBtn>
        </div>
      </header>

      {regenError && (
        <div className="mx-6 mt-4 p-3 bg-red-50 border border-red-200 text-red-800 rounded text-sm">
          ⚠ 重新生成失败: {regenError}
        </div>
      )}

      <div className="flex-1 overflow-y-auto px-6 py-4">
        {tab === 'minutes' && (
          <article className="max-w-3xl mx-auto prose prose-sm prose-headings:font-bold prose-headings:mt-6 prose-headings:mb-3 prose-h1:text-2xl prose-h2:text-lg prose-h2:border-b prose-h2:pb-1 prose-p:my-2 prose-ul:my-2 prose-li:my-0.5">
            {displayMd ? (
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{displayMd}</ReactMarkdown>
            ) : (
              <div className="text-gray-400 italic">
                这个会议还没有生成纪要。点 "重新生成纪要" 创建一份。
              </div>
            )}
          </article>
        )}

        {tab === 'transcript' && (
          <div className="max-w-3xl mx-auto space-y-2 font-mono text-sm">
            {detail.transcripts.length === 0 && (
              <div className="text-gray-400 italic">没有转写记录</div>
            )}
            {detail.transcripts.map((t) => (
              <div
                key={t.id}
                className={`flex gap-2 ${
                  t.speaker === 'system' ? 'text-blue-700' : 'text-green-700'
                }`}
              >
                <span className="text-gray-400 text-xs shrink-0 w-12">
                  +{(t.start_ms / 1000).toFixed(1)}s
                </span>
                <span className="font-bold shrink-0">
                  {t.speaker === 'system' ? '对方' : '我'}
                </span>
                <span>{t.text}</span>
              </div>
            ))}
          </div>
        )}

        {tab === 'suggestions' && (
          <div className="max-w-3xl mx-auto space-y-3">
            {detail.suggestions.length === 0 && (
              <div className="text-gray-400 italic">没有建议历史</div>
            )}
            {detail.suggestions.map((s) => (
              <div key={s.id} className="p-3 bg-gray-50 border border-gray-200 rounded">
                <div className="text-xs text-gray-500 mb-1">
                  {fmtTime(s.triggered_at)}
                  {s.trigger_type && <span className="ml-2">({s.trigger_type})</span>}
                </div>
                <div className="text-sm whitespace-pre-wrap">{s.content}</div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function TabBtn({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={`px-3 py-1 rounded transition ${
        active ? 'bg-blue-100 text-blue-700 font-medium' : 'text-gray-600 hover:bg-gray-100'
      }`}
    >
      {children}
    </button>
  );
}
