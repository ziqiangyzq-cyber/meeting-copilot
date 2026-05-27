import { useEffect, useState } from 'react';
import { listMeetings, MeetingSummary } from '../lib/tauri-bridge';

interface Props {
  onSelect: (meetingId: string) => void;
  onBack: () => void;
}

export function HistoryList({ onSelect, onBack }: Props) {
  const [meetings, setMeetings] = useState<MeetingSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

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
            <li key={m.id}>
              <button
                onClick={() => onSelect(m.id)}
                className="w-full text-left px-4 py-3 bg-white border border-gray-200 rounded hover:bg-blue-50 hover:border-blue-400 transition"
              >
                <div className="flex items-baseline gap-3">
                  <span className="font-bold text-gray-900">{m.name}</span>
                  {m.has_minutes && (
                    <span className="text-xs px-1.5 py-0.5 bg-green-100 text-green-700 rounded">
                      📝 有纪要
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
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
