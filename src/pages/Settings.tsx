import { useEffect, useState } from 'react';
import {
  getApiKeyStatus,
  getLlmStatus,
  getVoiceProcessing,
  saveAliyunOnly,
  saveMinimaxOnly,
  saveOpenaiCompat,
  setVoiceProcessing,
  testAliyunKey,
  testMinimaxKey,
  testOpenaiCompat,
  KeyStatus,
  LlmProvider,
  LlmStatus,
} from '../lib/tauri-bridge';

interface Props {
  onBack: () => void;
  isFirstLaunch?: boolean;
  onSaved?: () => void;
}

type TestState = 'idle' | 'testing' | 'ok' | 'fail';

export function Settings({ onBack, isFirstLaunch, onSaved }: Props) {
  const [status, setStatus] = useState<KeyStatus | null>(null);
  const [llmStatus, setLlmStatus] = useState<LlmStatus | null>(null);

  // Aliyun
  const [aliyun, setAliyun] = useState('');
  const [aliyunTest, setAliyunTest] = useState<TestState>('idle');
  const [aliyunTestMsg, setAliyunTestMsg] = useState<string | null>(null);

  // Provider
  const [provider, setProvider] = useState<LlmProvider>('minimax');

  // MiniMax
  const [minimax, setMinimax] = useState('');
  const [minimaxTest, setMinimaxTest] = useState<TestState>('idle');
  const [minimaxTestMsg, setMinimaxTestMsg] = useState<string | null>(null);

  // OpenAI-compat
  const [openaiBaseUrl, setOpenaiBaseUrl] = useState('');
  const [openaiModel, setOpenaiModel] = useState('');
  const [openaiApiKey, setOpenaiApiKey] = useState('');
  const [openaiTest, setOpenaiTest] = useState<TestState>('idle');
  const [openaiTestMsg, setOpenaiTestMsg] = useState<string | null>(null);

  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Mic voice processing toggle
  const [voiceProcEnabled, setVoiceProcEnabled] = useState(true);

  useEffect(() => {
    getApiKeyStatus().then(setStatus).catch((e) => setError(String(e)));
    getLlmStatus()
      .then((s) => {
        setLlmStatus(s);
        setProvider(s.provider);
        setOpenaiBaseUrl(s.current_base_url);
        setOpenaiModel(s.current_model);
      })
      .catch((e) => setError(String(e)));
    getVoiceProcessing().then(setVoiceProcEnabled).catch((e) => console.error(e));
  }, []);

  const handleToggleVoiceProc = async (next: boolean) => {
    setVoiceProcEnabled(next);
    try {
      await setVoiceProcessing(next);
    } catch (e) {
      console.error('setVoiceProcessing failed', e);
      setVoiceProcEnabled(!next); // revert
    }
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

  const handleTestOpenai = async () => {
    if (!openaiBaseUrl.trim() || !openaiModel.trim() || !openaiApiKey.trim()) {
      setOpenaiTest('fail');
      setOpenaiTestMsg('Base URL / 模型名 / Key 都要填');
      return;
    }
    setOpenaiTest('testing');
    setOpenaiTestMsg(null);
    try {
      await testOpenaiCompat(openaiBaseUrl.trim(), openaiModel.trim(), openaiApiKey.trim());
      setOpenaiTest('ok');
      setOpenaiTestMsg('调通了');
    } catch (e) {
      setOpenaiTest('fail');
      setOpenaiTestMsg(String(e));
    }
  };

  const refreshStatus = async () => {
    const [s, ls] = await Promise.all([getApiKeyStatus(), getLlmStatus()]);
    setStatus(s);
    setLlmStatus(ls);
  };

  const handleSave = async () => {
    setError(null);
    setSaved(false);

    const aliyunVal = aliyun.trim();
    const minimaxVal = minimax.trim();
    const openaiBaseVal = openaiBaseUrl.trim();
    const openaiModelVal = openaiModel.trim();
    const openaiKeyVal = openaiApiKey.trim();

    // Validation
    const needAliyun = !status?.aliyun_set;
    if (needAliyun && !aliyunVal) {
      setError('请填阿里 DashScope Key');
      return;
    }

    if (provider === 'minimax') {
      const needMinimax = !llmStatus?.minimax_set;
      if (needMinimax && !minimaxVal) {
        setError('请填 MiniMax Key');
        return;
      }
    } else {
      if (!openaiBaseVal || !openaiModelVal) {
        setError('Base URL 和 模型名 都要填');
        return;
      }
      const needOpenaiKey = !llmStatus?.openai_compat_set;
      if (needOpenaiKey && !openaiKeyVal) {
        setError('请填 API Key');
        return;
      }
    }

    // Test-failure confirm
    const failedTests: string[] = [];
    if (aliyunVal && aliyunTest === 'fail') failedTests.push('阿里');
    if (provider === 'minimax' && minimaxVal && minimaxTest === 'fail') failedTests.push('MiniMax');
    if (provider === 'openai_compat' && openaiKeyVal && openaiTest === 'fail') failedTests.push('OpenAI 兼容');
    if (failedTests.length > 0) {
      if (!confirm(`${failedTests.join('、')} 测试失败,确定要保存吗?`)) return;
    }

    setSaving(true);
    try {
      // 1. Aliyun (independent)
      if (aliyunVal) {
        await saveAliyunOnly(aliyunVal);
      }

      // 2. LLM provider config (backend preserves existing key when input is empty)
      if (provider === 'minimax') {
        await saveMinimaxOnly(minimaxVal);
      } else {
        await saveOpenaiCompat(openaiBaseVal, openaiModelVal, openaiKeyVal);
      }

      setSaved(true);
      await refreshStatus();
      setAliyun('');
      setMinimax('');
      setOpenaiApiKey('');
      setAliyunTest('idle');
      setMinimaxTest('idle');
      setOpenaiTest('idle');
      onSaved?.();
      setTimeout(() => setSaved(false), 1500);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const canBack =
    !isFirstLaunch ||
    (status?.aliyun_set && (llmStatus?.minimax_set || llmStatus?.openai_compat_set));

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

      <div className="mb-6 p-3 bg-gray-50 border border-gray-200 rounded text-xs text-gray-700">
        <strong>🔒 隐私说明</strong>
        <ul className="mt-1 ml-4 list-disc space-y-0.5">
          <li>不保存任何音频文件(实时转写,音频即丢)</li>
          <li>转写文字 → 阿里 DashScope(境内,有 DPA)</li>
          <li>AI 建议/纪要/翻译 → 你选的 LLM provider</li>
          <li>资料文件向量化 → 阿里 text-embedding-v3(向量入本地 SQLite,原文不出境)</li>
          <li>API Key 存本地 JSON 文件(0600 权限),不上云</li>
        </ul>
      </div>

      {isFirstLaunch && (
        <div className="mb-6 p-4 bg-blue-50 border border-blue-200 rounded text-sm text-blue-900">
          <p className="font-medium mb-2">这个工具需要 2 个东西才能工作:</p>
          <ul className="list-disc ml-5 space-y-1.5">
            <li>
              <strong>阿里 DashScope API Key</strong> — 实时语音转写 + 资料向量化
              <br />
              <span className="text-xs">
                注册:
                <code className="bg-blue-100 px-1">
                  https://bailian.console.aliyun.com/cn-beijing?tab=model#/api-key
                </code>
              </span>
            </li>
            <li>
              <strong>一个 LLM</strong>(AI 建议 + 纪要 + 翻译用),从以下任选:
              <ul className="list-disc ml-5 mt-1 space-y-0.5 text-xs">
                <li>
                  MiniMax(默认,需开通 <code className="bg-blue-100 px-1">MiniMax-M2.7-highspeed</code>,
                  <code className="bg-blue-100 px-1">https://platform.minimaxi.com</code>)
                </li>
                <li>OpenAI 兼容(DeepSeek / 阿里 Qwen / OpenAI / Ollama 本地等),填 base URL + 模型名 + key</li>
              </ul>
            </li>
          </ul>
          <p className="mt-3 text-xs">
            Key 保存在本地配置文件(
            <code className="bg-blue-100 px-1">~/Library/Application Support/com.efc.meeting-copilot/keys.json</code>
            ,owner-only 权限)。填好后点「测试」先验证一下再保存。
          </p>
        </div>
      )}

      <div className="space-y-6">
        {/* === 阿里 DashScope === */}
        <section className="border-b border-gray-200 pb-5">
          <h2 className="text-sm font-semibold text-gray-700 mb-3">
            🎙️ 阿里 DashScope(ASR + Embedding)
          </h2>
          <label className="block text-sm font-medium mb-1">
            阿里 DashScope API Key
            {status?.aliyun_set && <span className="ml-2 text-xs text-green-600">(已配置)</span>}
          </label>
          <div className="flex gap-2">
            <input
              type="password"
              value={aliyun}
              onChange={(e) => {
                setAliyun(e.target.value);
                setAliyunTest('idle');
                setAliyunTestMsg(null);
              }}
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
            申请:{' '}
            <a
              href="https://bailian.console.aliyun.com/cn-beijing?tab=model#/api-key"
              target="_blank"
              rel="noreferrer"
              className="text-blue-600 underline"
            >
              bailian.console.aliyun.com
            </a>{' '}
            → 创建 API Key + 开通 paraformer-realtime-v2 + text-embedding-v3
          </div>
        </section>

        {/* === LLM provider === */}
        <section>
          <h2 className="text-sm font-semibold text-gray-700 mb-3">
            🤖 LLM 模型(AI 建议 / 纪要 / 翻译)
          </h2>

          <div className="space-y-2 mb-4">
            <label className="flex items-start gap-2 cursor-pointer">
              <input
                type="radio"
                name="llm-provider"
                checked={provider === 'minimax'}
                onChange={() => setProvider('minimax')}
                className="mt-1"
              />
              <div className="flex-1">
                <div className="text-sm font-medium">
                  MiniMax(默认)
                  {llmStatus?.minimax_set && (
                    <span className="ml-2 text-xs text-green-600">(已配置)</span>
                  )}
                </div>
                <div className="text-xs text-gray-500">
                  国内厂商,使用 MiniMax-M2.7-highspeed,需在 platform.minimaxi.com 开通该模型
                </div>
              </div>
            </label>

            <label className="flex items-start gap-2 cursor-pointer">
              <input
                type="radio"
                name="llm-provider"
                checked={provider === 'openai_compat'}
                onChange={() => setProvider('openai_compat')}
                className="mt-1"
              />
              <div className="flex-1">
                <div className="text-sm font-medium">
                  自定义(OpenAI 兼容协议)
                  {llmStatus?.openai_compat_set && (
                    <span className="ml-2 text-xs text-green-600">(已配置)</span>
                  )}
                </div>
                <div className="text-xs text-gray-500">
                  DeepSeek / 阿里 Qwen / OpenAI / Ollama 本地…只要遵守 /v1/chat/completions SSE 协议都能用
                </div>
              </div>
            </label>
          </div>

          {provider === 'minimax' ? (
            <div>
              <label className="block text-sm font-medium mb-1">
                MiniMax API Key
                {llmStatus?.minimax_set && (
                  <span className="ml-2 text-xs text-green-600">(已配置)</span>
                )}
              </label>
              <div className="flex gap-2">
                <input
                  type="password"
                  value={minimax}
                  onChange={(e) => {
                    setMinimax(e.target.value);
                    setMinimaxTest('idle');
                    setMinimaxTestMsg(null);
                  }}
                  disabled={saving}
                  className="flex-1 px-3 py-2 border border-gray-300 rounded font-mono text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:bg-gray-100"
                  placeholder={
                    llmStatus?.minimax_set ? '••••••• (重新输入完整 key 以更新)' : 'sk-cp-...'
                  }
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
                申请:{' '}
                <a
                  href="https://platform.minimaxi.com"
                  target="_blank"
                  rel="noreferrer"
                  className="text-blue-600 underline"
                >
                  platform.minimaxi.com
                </a>{' '}
                → 开通 MiniMax-M2.7-highspeed
              </div>
            </div>
          ) : (
            <div className="space-y-3">
              <div>
                <label className="block text-sm font-medium mb-1">Base URL</label>
                <input
                  type="text"
                  value={openaiBaseUrl}
                  onChange={(e) => {
                    setOpenaiBaseUrl(e.target.value);
                    setOpenaiTest('idle');
                    setOpenaiTestMsg(null);
                  }}
                  disabled={saving}
                  className="w-full px-3 py-2 border border-gray-300 rounded font-mono text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:bg-gray-100"
                  placeholder="https://api.deepseek.com/v1"
                />
                <div className="text-xs text-gray-500 mt-1">
                  例如:DeepSeek <code className="bg-gray-100 px-1">https://api.deepseek.com/v1</code> · 阿里 Qwen{' '}
                  <code className="bg-gray-100 px-1">https://dashscope.aliyuncs.com/compatible-mode/v1</code> · OpenAI{' '}
                  <code className="bg-gray-100 px-1">https://api.openai.com/v1</code> · Ollama{' '}
                  <code className="bg-gray-100 px-1">http://localhost:11434/v1</code>。注意尾部不要加
                  /chat/completions。
                </div>
              </div>
              <div>
                <label className="block text-sm font-medium mb-1">模型名</label>
                <input
                  type="text"
                  value={openaiModel}
                  onChange={(e) => {
                    setOpenaiModel(e.target.value);
                    setOpenaiTest('idle');
                    setOpenaiTestMsg(null);
                  }}
                  disabled={saving}
                  className="w-full px-3 py-2 border border-gray-300 rounded font-mono text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:bg-gray-100"
                  placeholder="deepseek-chat"
                />
                <div className="text-xs text-gray-500 mt-1">
                  例如:<code className="bg-gray-100 px-1">deepseek-chat</code> ·{' '}
                  <code className="bg-gray-100 px-1">qwen-plus</code> ·{' '}
                  <code className="bg-gray-100 px-1">gpt-4o-mini</code> ·{' '}
                  <code className="bg-gray-100 px-1">llama3.1</code>
                </div>
              </div>
              <div>
                <label className="block text-sm font-medium mb-1">
                  API Key
                  {llmStatus?.openai_compat_set && (
                    <span className="ml-2 text-xs text-green-600">(已配置)</span>
                  )}
                </label>
                <div className="flex gap-2">
                  <input
                    type="password"
                    value={openaiApiKey}
                    onChange={(e) => {
                      setOpenaiApiKey(e.target.value);
                      setOpenaiTest('idle');
                      setOpenaiTestMsg(null);
                    }}
                    disabled={saving}
                    className="flex-1 px-3 py-2 border border-gray-300 rounded font-mono text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:bg-gray-100"
                    placeholder={
                      llmStatus?.openai_compat_set
                        ? '••••••• (重新输入完整 key 以更新)'
                        : 'sk-...'
                    }
                  />
                  <button
                    onClick={handleTestOpenai}
                    disabled={openaiTest === 'testing' || saving}
                    className="px-3 py-2 bg-gray-100 hover:bg-gray-200 text-gray-700 text-sm rounded disabled:opacity-50"
                  >
                    {openaiTest === 'testing' ? '测试中...' : '测试'}
                  </button>
                </div>
                {openaiTest === 'ok' && (
                  <div className="text-xs text-green-600 mt-1">✓ {openaiTestMsg}</div>
                )}
                {openaiTest === 'fail' && (
                  <div className="text-xs text-red-600 mt-1">✗ {openaiTestMsg}</div>
                )}
              </div>
            </div>
          )}
        </section>

        {/* === 麦克风处理 === */}
        <section className="space-y-2 pt-4 border-t">
          <h2 className="text-sm font-semibold text-gray-700 mb-3">🎙️ 麦克风处理</h2>
          <label className="flex items-start gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={voiceProcEnabled}
              onChange={(e) => handleToggleVoiceProc(e.target.checked)}
              className="mt-1"
            />
            <div className="text-sm">
              <div className="font-medium">启用 macOS 内置降噪 + 回声消除 + 自动音量</div>
              <div className="text-xs text-gray-500 mt-1">
                和 Zoom / FaceTime 用同一个底层(AVAudioEngine Voice Processing)。
                <br />
                ✅ 多数情况下提升转写质量(键盘/空调声减弱、扬声器漏音消除)。
                <br />
                ⚠️ 如果你发现轻声议论 / 远场说话被压掉,关掉这个开关。
                <br />
                修改后 <strong>下次会议生效</strong>。
              </div>
            </div>
          </label>
        </section>

        {error && (
          <div className="p-3 bg-red-50 border border-red-200 text-red-800 rounded text-sm">
            ⚠ {error}
          </div>
        )}

        <div className="flex items-center gap-3 pt-2">
          <button
            onClick={handleSave}
            disabled={saving}
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white rounded font-medium"
          >
            {saving ? '保存中...' : '保存'}
          </button>
          {saved && <span className="text-sm text-green-600">✓ 已保存</span>}
        </div>
      </div>
    </div>
  );
}
