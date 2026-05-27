import { useEffect, useState } from 'react';
import {
  getApiKeyStatus,
  saveApiKeys,
  testAliyunKey,
  testMinimaxKey,
  KeyStatus,
} from '../lib/tauri-bridge';

interface Props {
  onBack: () => void;
  isFirstLaunch?: boolean;
  onSaved?: () => void;
}

type TestState = 'idle' | 'testing' | 'ok' | 'fail';

export function Settings({ onBack, isFirstLaunch, onSaved }: Props) {
  const [status, setStatus] = useState<KeyStatus | null>(null);
  const [aliyun, setAliyun] = useState('');
  const [minimax, setMinimax] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  const [aliyunTest, setAliyunTest] = useState<TestState>('idle');
  const [aliyunTestMsg, setAliyunTestMsg] = useState<string | null>(null);
  const [minimaxTest, setMinimaxTest] = useState<TestState>('idle');
  const [minimaxTestMsg, setMinimaxTestMsg] = useState<string | null>(null);

  useEffect(() => {
    getApiKeyStatus().then(setStatus).catch((e) => setError(String(e)));
  }, []);

  const setAliyunInput = (v: string) => {
    setAliyun(v);
    setAliyunTest('idle');
    setAliyunTestMsg(null);
  };
  const setMinimaxInput = (v: string) => {
    setMinimax(v);
    setMinimaxTest('idle');
    setMinimaxTestMsg(null);
  };

  const handleTestAliyun = async () => {
    if (!aliyun.trim()) {
      setAliyunTest('fail');
      setAliyunTestMsg('请先输入 Key');
      return;
    }
    setAliyunTest('testing');
    setAliyunTestMsg(null);
    try {
      await testAliyunKey(aliyun.trim());
      setAliyunTest('ok');
      setAliyunTestMsg('Key 有效');
    } catch (e) {
      setAliyunTest('fail');
      setAliyunTestMsg(String(e));
    }
  };

  const handleTestMinimax = async () => {
    if (!minimax.trim()) {
      setMinimaxTest('fail');
      setMinimaxTestMsg('请先输入 Key');
      return;
    }
    setMinimaxTest('testing');
    setMinimaxTestMsg(null);
    try {
      await testMinimaxKey(minimax.trim());
      setMinimaxTest('ok');
      setMinimaxTestMsg('Key 有效');
    } catch (e) {
      setMinimaxTest('fail');
      setMinimaxTestMsg(String(e));
    }
  };

  const handleSave = async () => {
    setError(null);
    setSaved(false);
    if (!aliyun.trim() || !minimax.trim()) {
      setError('两个 Key 都要填(重新输入完整 Key 才会更新)');
      return;
    }
    if (aliyunTest === 'fail' || minimaxTest === 'fail') {
      if (!confirm('其中至少一个 Key 测试失败,确定要保存吗?')) {
        return;
      }
    }
    setSaving(true);
    try {
      await saveApiKeys(aliyun.trim(), minimax.trim());
      setSaved(true);
      const fresh = await getApiKeyStatus();
      setStatus(fresh);
      setAliyun('');
      setMinimax('');
      setAliyunTest('idle');
      setAliyunTestMsg(null);
      setMinimaxTest('idle');
      setMinimaxTestMsg(null);
      onSaved?.();
      setTimeout(() => setSaved(false), 1500);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const canBack = !isFirstLaunch || (status?.aliyun_set && status?.minimax_set);

  return (
    <div className="min-h-screen bg-white p-8 max-w-2xl mx-auto">
      <header className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold">
          {isFirstLaunch ? '👋 首次使用 — 配置 API Key' : '⚙️ 设置'}
        </h1>
        {canBack && (
          <button
            onClick={onBack}
            className="px-3 py-1.5 bg-gray-100 hover:bg-gray-200 text-gray-700 text-sm rounded"
          >
            返回
          </button>
        )}
      </header>

      {isFirstLaunch && (
        <div className="mb-6 p-4 bg-blue-50 border border-blue-200 rounded text-sm text-blue-900">
          <p className="font-medium mb-2">这个工具需要 2 个 API Key 才能工作:</p>
          <ul className="list-disc ml-5 space-y-1.5">
            <li><strong>阿里 DashScope</strong> — 实时语音转写 + 资料向量化<br/><span className="text-xs">注册:<code className="bg-blue-100 px-1">https://dashscope.console.aliyun.com</code></span></li>
            <li><strong>MiniMax</strong> — AI 建议 + 会议纪要 + 翻译<br/><span className="text-xs">注册:<code className="bg-blue-100 px-1">https://platform.minimaxi.com</code>,需要开通 <code className="bg-blue-100 px-1">MiniMax-M2.7-highspeed</code> 模型</span></li>
          </ul>
          <p className="mt-3 text-xs">两边都有免费额度。Key 保存在 macOS 钥匙串(系统加密),不出现在任何文件里。填好后建议点「测试」先验证一下再保存。</p>
        </div>
      )}

      <div className="space-y-5">
        <div>
          <label className="block text-sm font-medium mb-1">
            阿里 DashScope API Key
            {status?.aliyun_set && <span className="ml-2 text-xs text-green-600">(已配置)</span>}
          </label>
          <div className="flex gap-2">
            <input
              type="password"
              value={aliyun}
              onChange={(e) => setAliyunInput(e.target.value)}
              disabled={saving}
              className="flex-1 px-3 py-2 border border-gray-300 rounded font-mono text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:bg-gray-100"
              placeholder={status?.aliyun_set ? '••••••• (重新输入完整 key 以更新)' : 'sk-...'}
            />
            <button
              onClick={handleTestAliyun}
              disabled={aliyunTest === 'testing' || saving || !aliyun.trim()}
              className="px-3 py-2 bg-gray-100 hover:bg-gray-200 text-gray-700 text-sm rounded disabled:opacity-50"
            >
              {aliyunTest === 'testing' ? '测试中...' : '测试'}
            </button>
          </div>
          {aliyunTest === 'ok' && (
            <div className="text-xs text-green-600 mt-1">✓ {aliyunTestMsg}</div>
          )}
          {aliyunTest === 'fail' && (
            <div className="text-xs text-red-600 mt-1">✗ {aliyunTestMsg}</div>
          )}
          <div className="text-xs text-gray-500 mt-1">
            申请: <a href="https://dashscope.console.aliyun.com" target="_blank" rel="noreferrer" className="text-blue-600 underline">dashscope.console.aliyun.com</a> → 开通 paraformer-realtime-v2 + text-embedding-v3
          </div>
        </div>

        <div>
          <label className="block text-sm font-medium mb-1">
            MiniMax API Key
            {status?.minimax_set && <span className="ml-2 text-xs text-green-600">(已配置)</span>}
          </label>
          <div className="flex gap-2">
            <input
              type="password"
              value={minimax}
              onChange={(e) => setMinimaxInput(e.target.value)}
              disabled={saving}
              className="flex-1 px-3 py-2 border border-gray-300 rounded font-mono text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:bg-gray-100"
              placeholder={status?.minimax_set ? '••••••• (重新输入完整 key 以更新)' : 'sk-cp-...'}
            />
            <button
              onClick={handleTestMinimax}
              disabled={minimaxTest === 'testing' || saving || !minimax.trim()}
              className="px-3 py-2 bg-gray-100 hover:bg-gray-200 text-gray-700 text-sm rounded disabled:opacity-50"
            >
              {minimaxTest === 'testing' ? '测试中...' : '测试'}
            </button>
          </div>
          {minimaxTest === 'ok' && (
            <div className="text-xs text-green-600 mt-1">✓ {minimaxTestMsg}</div>
          )}
          {minimaxTest === 'fail' && (
            <div className="text-xs text-red-600 mt-1">✗ {minimaxTestMsg}</div>
          )}
          <div className="text-xs text-gray-500 mt-1">
            申请: <a href="https://platform.minimaxi.com" target="_blank" rel="noreferrer" className="text-blue-600 underline">platform.minimaxi.com</a> → 开通 MiniMax-M2.7-highspeed
          </div>
        </div>

        {error && (
          <div className="p-3 bg-red-50 border border-red-200 text-red-800 rounded text-sm">
            ⚠ {error}
          </div>
        )}

        <div className="flex items-center gap-3">
          <button
            onClick={handleSave}
            disabled={saving}
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white rounded font-medium"
          >
            {saving ? '保存中...' : '保存到钥匙串'}
          </button>
          {saved && <span className="text-sm text-green-600">✓ 已保存</span>}
        </div>
      </div>
    </div>
  );
}
