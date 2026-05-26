import { useState } from 'react';
import {
  MeetingDraft,
  createMeeting,
  startMeetingWithId,
  showFloating,
  hideFloating,
} from '../lib/tauri-bridge';
import { MeetingForm } from '../components/MeetingForm';
import { FileDropzone } from '../components/FileDropzone';

type Stage = 'form' | 'materials' | 'starting' | 'started';

export function Setup() {
  const [stage, setStage] = useState<Stage>('form');
  const [meetingId, setMeetingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [materialsReady, setMaterialsReady] = useState(false);

  const handleCreate = async (draft: MeetingDraft) => {
    setError(null);
    try {
      const id = await createMeeting(draft);
      setMeetingId(id);
      setMaterialsReady(true); // ready immediately — no files yet
      setStage('materials');
    } catch (e) {
      setError(String(e));
    }
  };

  const handleStart = async () => {
    if (!meetingId) return;
    setStage('starting');
    setError(null);
    try {
      await startMeetingWithId(meetingId);
      await showFloating();
      setStage('started');
    } catch (e) {
      setError(String(e));
      setStage('materials');
    }
  };

  return (
    <div className="min-h-screen bg-white p-8 max-w-2xl mx-auto">
      <h1 className="text-2xl font-bold mb-2">会议助理</h1>
      <p className="text-gray-600 mb-6 text-sm">EFC 会议 AI 助理 — 实时转写 + 智能建议</p>

      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 text-red-800 rounded text-sm">
          错误:{error}
        </div>
      )}

      {stage === 'form' && (
        <section>
          <h2 className="text-lg font-semibold mb-4">1. 新建会议</h2>
          <MeetingForm onSubmit={handleCreate} />
        </section>
      )}

      {stage === 'materials' && meetingId && (
        <section className="space-y-6">
          <div className="p-3 bg-blue-50 border border-blue-200 rounded text-sm text-blue-800">
            会议已创建:{meetingId.slice(0, 8)}...
          </div>

          <div>
            <h2 className="text-lg font-semibold mb-4">2. 上传会议资料(可选)</h2>
            <FileDropzone
              meetingId={meetingId}
              onAllReady={(anyIngested) => {
                // anyIngested is informational; always allow proceeding once files settle
                setMaterialsReady(true);
                void anyIngested;
              }}
            />
          </div>

          <div className="pt-4 border-t">
            <button
              onClick={handleStart}
              disabled={!materialsReady}
              className="px-6 py-2 bg-green-600 text-white rounded hover:bg-green-700 disabled:bg-gray-400 transition"
            >
              开始会议 ▶
            </button>
            <p className="text-xs text-gray-500 mt-2">
              {!materialsReady
                ? '等待资料索引完成...'
                : '可以直接开始,或继续拖入更多资料'}
            </p>
          </div>
        </section>
      )}

      {stage === 'starting' && (
        <section className="text-center py-12 text-gray-600">
          🔄 正在启动会议(请求屏幕录制 + 麦克风权限)...
        </section>
      )}

      {stage === 'started' && (
        <section className="text-center py-12">
          <div className="text-4xl mb-4">🎙️</div>
          <p className="text-lg font-semibold">会议进行中</p>
          <p className="text-sm text-gray-500 mt-2">
            浮窗已在桌面右下角。在浮窗里点"结束"可停止会议。
          </p>
          <button
            onClick={async () => {
              await hideFloating();
              // For testing: reset stage back to form for another meeting
              setStage('form');
              setMeetingId(null);
              setMaterialsReady(false);
            }}
            className="mt-6 px-4 py-2 bg-gray-200 text-gray-700 rounded hover:bg-gray-300 text-sm"
          >
            新建另一个会议
          </button>
        </section>
      )}
    </div>
  );
}
