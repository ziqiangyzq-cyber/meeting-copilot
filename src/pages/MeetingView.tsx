import { useEffect, useRef, useState } from 'react';
import {
  TranscriptEvent,
  onTranscript,
  onSuggestionToken,
  onSuggestionComplete,
  onSuggestionError,
  triggerSuggestion,
  stopMeeting,
  hideFloating,
} from '../lib/tauri-bridge';

interface CompletedSuggestion {
  text: string;
  timestamp: number;
}

interface Props {
  onEnd: () => void;
}

export function MeetingView({ onEnd }: Props) {
  const [transcripts, setTranscripts] = useState<TranscriptEvent[]>([]);
  const [suggestions, setSuggestions] = useState<CompletedSuggestion[]>([]);
  const [currentStream, setCurrentStream] = useState<string>('');
  const [error, setError] = useState<string | null>(null);
  const accumRef = useRef<string>('');
  const transcriptEndRef = useRef<HTMLDivElement>(null);

  // transcript subscription
  useEffect(() => {
    let unlisten: (() => void) | undefined;
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
    }).then((fn) => { unlisten = fn; });
    return () => unlisten?.();
  }, []);

  // suggestion subscription
  useEffect(() => {
    let unlistenToken: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;

    onSuggestionToken((token) => {
      accumRef.current += token;
      setCurrentStream(accumRef.current);
    }).then((fn) => { unlistenToken = fn; });

    onSuggestionComplete(() => {
      if (accumRef.current.trim()) {
        const text = accumRef.current;
        setSuggestions((prev) => [{ text, timestamp: Date.now() }, ...prev]);
      }
      accumRef.current = '';
      setCurrentStream('');
    }).then((fn) => { unlistenDone = fn; });

    onSuggestionError((err) => {
      setError(err);
      accumRef.current = '';
      setCurrentStream('');
    }).then((fn) => { unlistenError = fn; });

    return () => {
      unlistenToken?.();
      unlistenDone?.();
      unlistenError?.();
    };
  }, []);

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
    await hideFloating();
    onEnd();
  };

  const handleTrigger = async () => {
    try {
      await triggerSuggestion();
    } catch (e) {
      console.error('trigger_suggestion failed', e);
    }
  };

  const formatTime = (ts: number) => {
    const d = new Date(ts);
    return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}:${d.getSeconds().toString().padStart(2, '0')}`;
  };

  return (
    <div className="h-screen flex flex-col bg-white">
      <header className="px-6 py-3 border-b flex items-center gap-3 shrink-0">
        <h1 className="text-lg font-bold">会议进行中</h1>
        <div className="text-xs text-gray-500">{transcripts.length} 条转写 · {suggestions.length} 条建议</div>
        <div className="flex-1" />
        <button
          onClick={handleTrigger}
          className="px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white text-sm rounded"
        >
          ⌘⇧M 召唤建议
        </button>
        <button
          onClick={handleStop}
          className="px-4 py-1.5 bg-red-600 hover:bg-red-700 text-white text-sm font-bold rounded shadow"
        >
          结束会议
        </button>
      </header>

      <div className="flex-1 grid grid-cols-2 overflow-hidden">
        <section className="overflow-y-auto border-r p-4 space-y-2">
          <h2 className="font-bold text-sm sticky top-0 bg-white pb-2 border-b">实时转写</h2>
          {transcripts.length === 0 && (
            <div className="text-gray-400 italic text-sm">等待转写...</div>
          )}
          {transcripts.map((t, i) => (
            <div key={i} className={`flex gap-2 text-sm font-mono ${t.source === 'system' ? 'text-blue-700' : 'text-green-700'}`}>
              <span className="font-bold shrink-0">{t.source === 'system' ? '对方' : '我'}</span>
              <span className="break-words">{t.text}{!t.is_final && <span className="text-gray-400">…</span>}</span>
            </div>
          ))}
          <div ref={transcriptEndRef} />
        </section>

        <section className="overflow-y-auto p-4 space-y-3">
          <h2 className="font-bold text-sm sticky top-0 bg-white pb-2 border-b">建议历史</h2>

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
        <span>浮窗在右侧 · ⌘⇧M 召唤</span>
      </footer>
    </div>
  );
}
