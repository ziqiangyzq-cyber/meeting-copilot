import { useEffect, useState } from 'react';
import {
  onSuggestionToken,
  onSuggestionComplete,
  onSuggestionError,
  triggerSuggestion,
} from '../lib/tauri-bridge';

interface State {
  text: string;
  isStreaming: boolean;
  error: string | null;
}

export function SuggestionCard() {
  const [state, setState] = useState<State>({
    text: '',
    isStreaming: false,
    error: null,
  });

  useEffect(() => {
    let unlistenToken: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;

    onSuggestionToken((token) => {
      setState((prev) => {
        if (!prev.isStreaming) {
          // First token of a new stream — reset text
          return { text: token, isStreaming: true, error: null };
        }
        return { ...prev, text: prev.text + token };
      });
    }).then((fn) => {
      unlistenToken = fn;
    });

    onSuggestionComplete(() => {
      setState((prev) => ({ ...prev, isStreaming: false }));
    }).then((fn) => {
      unlistenDone = fn;
    });

    onSuggestionError((err) => {
      setState((prev) => ({ ...prev, isStreaming: false, error: err }));
    }).then((fn) => {
      unlistenError = fn;
    });

    return () => {
      unlistenToken?.();
      unlistenDone?.();
      unlistenError?.();
    };
  }, []);

  const handleManualTrigger = async () => {
    try {
      await triggerSuggestion();
    } catch (e) {
      setState((prev) => ({ ...prev, error: String(e), isStreaming: false }));
    }
  };

  return (
    <div className="px-3 py-2 text-white">
      {state.error && (
        <div className="text-orange-300 text-[10px] mb-1">⚠ {state.error}</div>
      )}

      {state.text === '' && !state.isStreaming && !state.error && (
        <div className="text-white/40 text-[11px] italic">
          按 ⌘⇧M 召唤建议,或等自动建议(每 20s)
        </div>
      )}

      {state.text && (
        <div className="text-[12px] leading-snug whitespace-pre-wrap break-words">
          {state.text}
          {state.isStreaming && (
            <span className="inline-block w-1 h-3 bg-white/70 ml-0.5 animate-pulse" />
          )}
        </div>
      )}

      <div className="mt-1 flex items-center gap-2">
        <button
          onClick={handleManualTrigger}
          disabled={state.isStreaming}
          className="text-[10px] text-white/60 hover:text-white/90 disabled:text-white/30"
        >
          ⌘⇧M 召唤
        </button>
        {state.isStreaming && (
          <span className="text-[10px] text-white/50">生成中...</span>
        )}
      </div>
    </div>
  );
}
