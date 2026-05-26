import { useEffect, useState } from 'react';
import { TranscriptEvent, startMeeting, stopMeeting, onTranscript } from './lib/tauri-bridge';
import { TranscriptView } from './components/TranscriptView';
import './App.css';

export default function App() {
  const [items, setItems] = useState<TranscriptEvent[]>([]);
  const [isRunning, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    onTranscript((evt) => {
      setItems((prev) => {
        // Strategy: if a partial transcript arrives, replace the last item if it's
        // also a partial from the same source (in-place update). Otherwise append.
        // When is_final is true, the item becomes immutable history.
        const last = prev[prev.length - 1];
        if (last && !last.is_final && last.source === evt.source) {
          const next = prev.slice(0, -1);
          next.push(evt);
          return next;
        }
        return [...prev, evt];
      });
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  const handleStart = async () => {
    setError(null);
    try {
      await startMeeting();
      setRunning(true);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleStop = async () => {
    try {
      await stopMeeting();
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="min-h-screen bg-white p-8">
      <h1 className="text-2xl font-bold mb-4">会议助理 — Plan 1 验证</h1>

      <div className="mb-4 flex gap-2 items-center">
        {!isRunning ? (
          <button
            onClick={handleStart}
            className="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 transition"
          >
            开始会议
          </button>
        ) : (
          <button
            onClick={handleStop}
            className="px-4 py-2 bg-red-600 text-white rounded hover:bg-red-700 transition"
          >
            结束会议
          </button>
        )}
        <span className="px-3 py-2 text-sm text-gray-600">
          状态:{isRunning ? '🟢 进行中' : '⚪ 空闲'}
        </span>
      </div>

      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 text-red-800 rounded text-sm">
          错误:{error}
        </div>
      )}

      <TranscriptView items={items} />
    </div>
  );
}
