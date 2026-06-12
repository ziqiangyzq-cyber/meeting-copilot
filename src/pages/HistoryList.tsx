import { useEffect, useState } from 'react';
import { deleteMeeting, listMeetings, MeetingSummary } from '../lib/tauri-bridge';

interface Props {
  onSelect: (meetingId: string) => void;
  onMerge: (meetingIds: string[]) => void;
  onBack: () => void;
}

const TEMPLATE_BADGE: Record<string, string> = {
  technical_review: '🔍 评审',
  coordination: '🔗 协调',
  field_discussion: '🏗️ 现场',
};

export function HistoryList({ onSelect, onMerge, onBack }: Props) {
  const [meetings, setMeetings] = useState<MeetingSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const toggleSelect = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  // Merge in chronological order (oldest segment first) regardless of click order.
  // Requires ≥2 segments — a single selection is just normal generation and
  // should go through the meeting's own detail page instead.
  const handleMerge = () => {
    const ordered = meetings
      .filter((m) => selected.has(m.id))
      .sort((a, b) => a.started_at - b.started_at);
    if (ordered.length < 2) return;
    const target = ordered[0];
    if (target.has_minutes) {
      const ok = confirm(
        `最早一段「${target.name}」已有纪要。\n\n合并生成的纪要会保存为它的最新版本(旧版本保留在数据库,但界面默认显示最新版)。\n\n继续?`,
      );
      if (!ok) return;
    }
    onMerge(ordered.map((m) => m.id));
  };

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    listMeetings()
      .then((m) => {
        if (!cancelled) {
          setMeetings(m);
          setLoading(false);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setError(String(e));
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const fmtDate = (ms: number) => {
    const d = new Date(ms);
    return `${d.getFullYear()}-${(d.getMonth() + 1).toString().padStart(2, '0')}-${d
      .getDate()
      .toString()
      .padStart(2, '0')} ${d.getHours().toString().padStart(2, '0')}:${d
      .getMinutes()
      .toString()
      .padStart(2, '0')}`;
  };

  const handleDelete = async (id: string, name: string) => {
    if (!confirm(`确定删除会议 "${name}"?\n\n会删除全部转写、建议、资料、纪要,不可恢复。`)) return;
    setDeleting(id);
    try {
      await deleteMeeting(id);
      setMeetings((prev) => prev.filter((m) => m.id !== id));
    } catch (e) {
      alert(`删除失败: ${e}`);
    } finally {
      setDeleting(null);
    }
  };

  const fmtDuration = (ms: number | null) => {
    if (!ms || ms <= 0) return '—';
    const secs = Math.floor(ms / 1000);
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    if (h > 0) return `${h}h${m}m`;
    return `${m}m`;
  };

  return (
    <div className="min-h-screen bg-white p-8 max-w-3xl mx-auto">
      <header className="flex items-center gap-4 mb-6">
        <h1 className="text-2xl font-bold">📋 历史会议</h1>
        <div className="flex-1" />
        <button
          onClick={onBack}
          className="px-3 py-1.5 bg-gray-100 hover:bg-gray-200 text-gray-700 text-sm rounded"
        >
          ← 返回
        </button>
      </header>

      {!loading && meetings.length > 0 && (
        <p className="text-xs text-gray-500 mb-3">
          勾选多段记录(例如会议中断后重开的几段)可合并生成一份纪要。
        </p>
      )}

      {selected.size > 0 && (
        <div className="sticky top-0 z-10 mb-3 flex items-center gap-3 px-4 py-2 bg-blue-600 text-white rounded shadow">
          <span className="text-sm font-medium">已选 {selected.size} 段</span>
          <div className="flex-1" />
          <button
            onClick={() => setSelected(new Set())}
            className="px-3 py-1.5 bg-blue-500 hover:bg-blue-400 text-white text-sm rounded"
          >
            清除
          </button>
          <button
            onClick={handleMerge}
            disabled={selected.size < 2}
            className="px-4 py-1.5 bg-white text-blue-700 hover:bg-blue-50 text-sm font-bold rounded disabled:opacity-60 disabled:cursor-not-allowed"
            title={selected.size < 2 ? '至少勾选 2 段才能合并;单段纪要请直接点开那条记录' : undefined}
          >
            {selected.size >= 2 ? `合并生成纪要 (${selected.size})` : '再勾 1 段以合并'}
          </button>
        </div>
      )}

      {loading && <div className="text-gray-400">加载中...</div>}

      {error && (
        <div className="p-3 bg-red-50 border border-red-200 text-red-800 rounded text-sm">
          ⚠ {error}
        </div>
      )}

      {!loading && !error && meetings.length === 0 && (
        <div className="text-gray-400 italic text-center py-12">还没有历史会议</div>
      )}

      {!loading && meetings.length > 0 && (
        <ul className="space-y-2">
          {meetings.map((m) => (
            <li key={m.id} className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={selected.has(m.id)}
                onChange={() => toggleSelect(m.id)}
                className="shrink-0 w-4 h-4 accent-blue-600 cursor-pointer"
                title="勾选以合并生成纪要"
              />
              <button
                onClick={() => onSelect(m.id)}
                className="flex-1 text-left px-4 py-3 bg-white border border-gray-200 rounded hover:bg-blue-50 hover:border-blue-400 transition"
              >
                <div className="flex items-baseline gap-3">
                  <span className="font-bold text-gray-900">{m.name}</span>
                  {m.has_minutes && (
                    <span className="text-xs px-1.5 py-0.5 bg-green-100 text-green-700 rounded">
                      📝 有纪要
                    </span>
                  )}
                  {m.template_id && TEMPLATE_BADGE[m.template_id] && (
                    <span className="text-xs px-1.5 py-0.5 bg-purple-100 text-purple-700 rounded">
                      {TEMPLATE_BADGE[m.template_id]}
                    </span>
                  )}
                </div>
                <div className="text-xs text-gray-500 mt-1 flex gap-4 flex-wrap">
                  <span>{fmtDate(m.started_at)}</span>
                  <span>时长 {fmtDuration(m.duration_ms)}</span>
                  {m.project_ref && <span>项目 {m.project_ref}</span>}
                  {m.purpose && <span>{m.purpose}</span>}
                  <span>
                    {m.transcript_count} 条转写 · {m.suggestion_count} 条建议
                  </span>
                </div>
              </button>
              <button
                onClick={() => handleDelete(m.id, m.name)}
                disabled={deleting === m.id}
                className="shrink-0 px-3 py-2 text-gray-400 hover:text-red-600 hover:bg-red-50 rounded disabled:opacity-50"
                title="删除会议"
              >
                🗑️
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
