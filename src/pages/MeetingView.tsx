import { useEffect, useRef, useState } from 'react';
import {
  TranscriptEvent,
  onTranscript,
  onSuggestionToken,
  onSuggestionComplete,
  onSuggestionError,
  setSuggestionsEnabled,
  stopMeeting,
  translateText,
  updateFocusPoints,
  updateMeetingNotes,
  restartMic,
  getVoiceProcessing,
  setVoiceProcessing,
} from '../lib/tauri-bridge';

interface CompletedSuggestion {
  text: string;
  timestamp: number;
}

interface Props {
  meetingId: string;
  initialFocusPoints?: string;
  onEnd: () => void;
}

function isLikelyEnglish(text: string): boolean {
  if (!text) return false;
  // If text has any CJK characters, it's not English-only
  if (/[一-鿿]/.test(text)) return false;
  // Must have at least some Latin alphabet characters to count
  return /[a-zA-Z]/.test(text);
}

export function MeetingView({ meetingId, initialFocusPoints, onEnd }: Props) {
  const [transcripts, setTranscripts] = useState<TranscriptEvent[]>([]);
  const [suggestions, setSuggestions] = useState<CompletedSuggestion[]>([]);
  const [currentStream, setCurrentStream] = useState<string>('');
  const [error, setError] = useState<string | null>(null);
  const [translations, setTranslations] = useState<Record<number, string>>({});
  const [translating, setTranslating] = useState<Set<number>>(new Set());
  const [suggestEnabled, setSuggestEnabled] = useState<boolean>(() => {
    const stored = localStorage.getItem('suggestEnabled');
    return stored === null ? true : stored === 'true';
  });
  const [focus, setFocus] = useState<string>(initialFocusPoints || '');
  const [notes, setNotes] = useState<string>('');
  const [vpEnabled, setVpEnabled] = useState<boolean>(true);
  const focusTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const notesTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const accumRef = useRef<string>('');
  const transcriptEndRef = useRef<HTMLDivElement>(null);
  const translationDispatched = useRef<Set<number>>(new Set());

  // Debounced save of focus_points to DB (500ms after last keystroke).
  // Engine re-reads meta on each generate so the next suggestion picks up the change.
  const handleFocusChange = (val: string) => {
    setFocus(val);
    if (focusTimer.current) clearTimeout(focusTimer.current);
    focusTimer.current = setTimeout(() => {
      updateFocusPoints(meetingId, val).catch((e) =>
        console.error('updateFocusPoints failed', e),
      );
    }, 500);
  };

  // Debounced save of quick notes (500ms after last keystroke).
  // Notes feed into the minutes generator as user anchors.
  const handleNotesChange = (val: string) => {
    setNotes(val);
    if (notesTimer.current) clearTimeout(notesTimer.current);
    notesTimer.current = setTimeout(() => {
      updateMeetingNotes(meetingId, val).catch((e) =>
        console.error('updateMeetingNotes failed', e),
      );
    }, 500);
  };

  // Cleanup any pending debounce on unmount so stale saves don't fire
  useEffect(() => {
    return () => {
      if (focusTimer.current) clearTimeout(focusTimer.current);
      if (notesTimer.current) clearTimeout(notesTimer.current);
    };
  }, []);

  // Persist toggle to localStorage
  useEffect(() => {
    localStorage.setItem('suggestEnabled', String(suggestEnabled));
  }, [suggestEnabled]);

  // Sync toggle state to backend (covers initial mount + subsequent flips)
  useEffect(() => {
    setSuggestionsEnabled(suggestEnabled).catch((e) =>
      console.error('set_suggestions_enabled failed', e),
    );
  }, [suggestEnabled]);

  // Initial load: read current voice-processing pref from backend (same source as Settings)
  useEffect(() => {
    getVoiceProcessing()
      .then(setVpEnabled)
      .catch((e) => console.error('getVoiceProcessing failed', e));
  }, []);

  const handleToggleVp = async () => {
    const next = !vpEnabled;
    setVpEnabled(next);
    try {
      await setVoiceProcessing(next);
    } catch (e) {
      console.error('toggle vp failed', e);
      setVpEnabled(!next); // revert on failure
    }
  };

  // transcript subscription
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let unmounted = false;
    onTranscript((evt) => {
      setTranscripts((prev) => {
        const last = prev[prev.length - 1];
        if (last && !last.is_final && last.source === evt.source) {
          const next = prev.slice(0, -1);
          next.push(evt);
          return next;
        }
        return [...prev, evt];
      });
    }).then((fn) => {
      if (unmounted) fn();
      else unlisten = fn;
    });
    return () => {
      unmounted = true;
      unlisten?.();
    };
  }, []);

  // suggestion subscription
  useEffect(() => {
    let unlistenToken: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;
    let unmounted = false;

    onSuggestionToken((token) => {
      accumRef.current += token;
      setCurrentStream(accumRef.current);
    }).then((fn) => {
      if (unmounted) fn();
      else unlistenToken = fn;
    });

    onSuggestionComplete(() => {
      if (accumRef.current.trim()) {
        const text = accumRef.current;
        setSuggestions((prev) => [{ text, timestamp: Date.now() }, ...prev]);
      }
      accumRef.current = '';
      setCurrentStream('');
    }).then((fn) => {
      if (unmounted) fn();
      else unlistenDone = fn;
    });

    onSuggestionError((err) => {
      setError(err);
      accumRef.current = '';
      setCurrentStream('');
    }).then((fn) => {
      if (unmounted) fn();
      else unlistenError = fn;
    });

    return () => {
      unmounted = true;
      unlistenToken?.();
      unlistenDone?.();
      unlistenError?.();
    };
  }, []);

  // Auto-dispatch translation for newly-final English transcripts
  useEffect(() => {
    transcripts.forEach((t, i) => {
      if (!t.is_final) return;
      if (translationDispatched.current.has(i)) return;
      if (!isLikelyEnglish(t.text)) return;
      translationDispatched.current.add(i);
      setTranslating((s) => {
        const next = new Set(s);
        next.add(i);
        return next;
      });
      const text = t.text;
      translateText(text).then((zh) => {
        setTranslations((m) => ({ ...m, [i]: zh }));
        setTranslating((s) => {
          const next = new Set(s);
          next.delete(i);
          return next;
        });
      }).catch((e) => {
        console.error('translate failed for idx', i, e);
        setTranslating((s) => {
          const next = new Set(s);
          next.delete(i);
          return next;
        });
      });
    });
  }, [transcripts]);

  // auto-scroll transcript to bottom
  useEffect(() => {
    transcriptEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [transcripts.length]);

  const handleStop = async () => {
    try {
      await stopMeeting();
    } catch (e) {
      console.error('stop_meeting failed', e);
    }
    onEnd();
  };

  const formatTime = (ts: number) => {
    const d = new Date(ts);
    return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}:${d.getSeconds().toString().padStart(2, '0')}`;
  };

  return (
    <div className="h-screen flex flex-col bg-white">
      <header className="px-6 py-3 border-b flex items-center gap-3 shrink-0">
        <h1 className="text-lg font-bold">会议进行中</h1>
        <span
          className="text-xs text-gray-500 flex items-center gap-1"
          title="此 app 不保存任何音频文件,只把音频实时流式发到阿里 Paraformer 转写后立即丢弃。"
        >
          🔒 不录音
        </span>
        <div className="text-xs text-gray-500">{transcripts.length} 条转写 · {suggestions.length} 条建议</div>
        <div className="flex-1" />
        <button
          onClick={async () => {
            try { await restartMic(); } catch (e) { console.error(e); }
          }}
          className="px-3 py-1.5 bg-gray-100 hover:bg-gray-200 text-gray-700 text-sm rounded"
          title="切换麦克风设备后如果没自动生效,点这个手动重连"
        >
          🎙️ 重连麦
        </button>
        <button
          onClick={handleToggleVp}
          className={`px-3 py-1.5 text-sm rounded transition ${
            vpEnabled
              ? 'bg-blue-50 hover:bg-blue-100 text-blue-700'
              : 'bg-gray-100 hover:bg-gray-200 text-gray-600'
          }`}
          title={
            vpEnabled
              ? '麦克风降噪开启中 (回声消除 + 降噪 + AGC)。点击关闭。'
              : '麦克风降噪已关闭。点击开启。'
          }
        >
          {vpEnabled ? '🎙️ 降噪开' : '🎙️ 降噪关'}
        </button>
        <button
          onClick={() => setSuggestEnabled(!suggestEnabled)}
          className={`px-3 py-1.5 text-white text-sm font-medium rounded transition ${
            suggestEnabled
              ? 'bg-blue-600 hover:bg-blue-700'
              : 'bg-gray-400 hover:bg-gray-500'
          }`}
          title={suggestEnabled ? 'AI 建议开启中 (点击关闭)' : 'AI 建议已关闭 (点击开启)'}
        >
          AI 建议: {suggestEnabled ? '开' : '关'}
        </button>
        <button
          onClick={handleStop}
          className="px-4 py-1.5 bg-red-600 hover:bg-red-700 text-white text-sm font-bold rounded shadow"
        >
          结束会议
        </button>
      </header>

      <div className="px-6 py-2 border-b bg-yellow-50 flex items-center gap-2 shrink-0">
        <span className="text-xs text-yellow-800 shrink-0 font-medium">💡 重点关注:</span>
        <input
          type="text"
          value={focus}
          onChange={(e) => handleFocusChange(e.target.value)}
          placeholder="点这里临时添加 AI 要重点帮你关注的事项(自动保存,下一条建议起生效)"
          className="flex-1 px-2 py-1 bg-transparent text-sm placeholder-gray-400 focus:outline-none focus:bg-white focus:px-3 focus:rounded focus:border focus:border-yellow-300"
        />
      </div>

      <div className="flex-1 grid grid-cols-2 overflow-hidden">
        <section className="overflow-y-auto border-r p-4 space-y-2">
          <h2 className="font-bold text-sm sticky top-0 bg-white pb-2 border-b">实时转写</h2>
          {transcripts.length === 0 && (
            <div className="text-gray-400 italic text-sm">等待转写...</div>
          )}
          {transcripts.map((t, i) => (
            <div key={i} className="flex flex-col gap-0.5">
              <div className={`flex gap-2 text-sm font-mono ${t.source === 'system' ? 'text-blue-700' : 'text-green-700'}`}>
                <span className="font-bold shrink-0">{t.source === 'system' ? '对方' : '我'}</span>
                <span className="break-words">{t.text}{!t.is_final && <span className="text-gray-400">…</span>}</span>
              </div>
              {translating.has(i) && (
                <div className="ml-8 text-xs text-gray-400 italic">翻译中...</div>
              )}
              {translations[i] && (
                <div className="ml-8 text-xs text-gray-500 italic">↳ {translations[i]}</div>
              )}
            </div>
          ))}
          <div ref={transcriptEndRef} />
        </section>

        <section className="overflow-y-auto p-4 space-y-3">
          <h2 className="font-bold text-sm pb-2 border-b">
            📝 快速笔记{' '}
            <span className="text-xs text-gray-400 font-normal">(纪要会围绕这些展开)</span>
          </h2>
          <textarea
            value={notes}
            onChange={(e) => handleNotesChange(e.target.value)}
            placeholder={'开会时随手敲关键词、决策点、待跟进...\n\n例:\n• 对方接受了 211 万\n• 下周三 demo\n• 防火封堵规范要再核对'}
            rows={5}
            className="w-full px-3 py-2 border border-gray-300 rounded text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none"
          />

          <h2 className="font-bold text-sm pb-2 border-b mt-4">建议历史</h2>

          {currentStream && (
            <div className="p-3 bg-yellow-50 border border-yellow-200 rounded">
              <div className="text-xs text-yellow-700 mb-1">生成中...</div>
              <div className="text-sm whitespace-pre-wrap">
                {currentStream}
                <span className="inline-block w-1 h-3 bg-gray-500 ml-0.5 animate-pulse" />
              </div>
            </div>
          )}

          {suggestions.length === 0 && !currentStream && (
            <div className="text-gray-400 italic text-sm">还没有建议(每 20s 自动出一条)</div>
          )}

          {suggestions.map((s, i) => (
            <div key={i} className="p-3 bg-gray-50 border border-gray-200 rounded">
              <div className="text-xs text-gray-500 mb-1">{formatTime(s.timestamp)}</div>
              <div className="text-sm whitespace-pre-wrap leading-relaxed">{s.text}</div>
            </div>
          ))}
        </section>
      </div>

      <footer className="px-4 py-2 border-t bg-gray-50 text-xs text-gray-600 flex items-center gap-4 shrink-0">
        <span>🟢 会议进行中</span>
        <span>转写 {transcripts.length} 条</span>
        <span>建议 {suggestions.length} 条</span>
        {error && <span className="text-red-600">⚠ {error}</span>}
        <div className="flex-1" />
        <span>AI 建议 {suggestEnabled ? '开启' : '已关闭'}</span>
      </footer>
    </div>
  );
}
