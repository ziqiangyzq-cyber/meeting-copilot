import { useEffect, useState } from 'react';
import { MeetingDraft, MeetingTemplate, listTemplates } from '../lib/tauri-bridge';

interface Props {
  onSubmit: (draft: MeetingDraft) => void;
  disabled?: boolean;
}

const DEFAULT_FOCUS_PLACEHOLDER =
  '开会前在这里写本次特别关注的技术点,AI 会围绕这些给提示。\n例:防火分区合规性 / 节点构造的耐久性 / 跟结构院的接口边界 / 核对图纸 vs 模型一致性';

export function MeetingForm({ onSubmit, disabled }: Props) {
  const [name, setName] = useState('');
  const [projectRef, setProjectRef] = useState('');
  const [purpose, setPurpose] = useState('');
  const [participants, setParticipants] = useState('');
  const [focusPoints, setFocusPoints] = useState('');
  const [templates, setTemplates] = useState<MeetingTemplate[]>([]);
  const [selectedTemplateId, setSelectedTemplateId] = useState<string>('default');

  useEffect(() => {
    listTemplates()
      .then(setTemplates)
      .catch((e) => console.error('listTemplates failed', e));
  }, []);

  // When template changes, prefill purpose IF user hasn't typed anything yet.
  // Don't overwrite user's typed focus_points — only the placeholder changes.
  useEffect(() => {
    const tpl = templates.find((t) => t.id === selectedTemplateId);
    if (!tpl) return;
    if (!purpose.trim() && tpl.default_purpose) {
      setPurpose(tpl.default_purpose);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedTemplateId, templates]);

  const currentFocusPlaceholder =
    templates.find((t) => t.id === selectedTemplateId)?.focus_placeholder ||
    DEFAULT_FOCUS_PLACEHOLDER;

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    onSubmit({
      name: name.trim(),
      project_ref: projectRef.trim() || undefined,
      purpose: purpose.trim() || undefined,
      participants: participants.trim() || undefined,
      focus_points: focusPoints.trim() || undefined,
      template_id: selectedTemplateId !== 'default' ? selectedTemplateId : undefined,
    });
  };

  const canSubmit = !!name.trim() && !disabled;

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      <div>
        <label className="block text-sm font-medium mb-1">
          会议类型(可选)
        </label>
        <select
          value={selectedTemplateId}
          onChange={(e) => setSelectedTemplateId(e.target.value)}
          disabled={disabled}
          className="w-full px-3 py-2 border border-gray-300 rounded focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:bg-gray-100"
        >
          {templates.length === 0 && <option value="default">默认(技术会议通用)</option>}
          {templates.map((t) => (
            <option key={t.id} value={t.id}>
              {t.display_name}
            </option>
          ))}
        </select>
        <div className="text-xs text-gray-500 mt-1">
          模板会预填"会议目的",并切换纪要结构。选"默认"= 技术会议通用风格。
        </div>
      </div>

      <div>
        <label className="block text-sm font-medium mb-1">
          会议名 <span className="text-red-500">*</span>
        </label>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          disabled={disabled}
          className="w-full px-3 py-2 border border-gray-300 rounded focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:bg-gray-100"
          placeholder=""
          required
        />
      </div>

      <div>
        <label className="block text-sm font-medium mb-1 text-gray-700">
          关联项目(可选)
        </label>
        <input
          type="text"
          value={projectRef}
          onChange={(e) => setProjectRef(e.target.value)}
          disabled={disabled}
          className="w-full px-3 py-2 border border-gray-300 rounded focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:bg-gray-100"
          placeholder=""
        />
      </div>

      <div>
        <label className="block text-sm font-medium mb-1 text-gray-700">
          会议目的(可选)
        </label>
        <input
          type="text"
          value={purpose}
          onChange={(e) => setPurpose(e.target.value)}
          disabled={disabled}
          className="w-full px-3 py-2 border border-gray-300 rounded focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:bg-gray-100"
          placeholder="模板会自动预填,你也可以自由填写"
        />
      </div>

      <div>
        <label className="block text-sm font-medium mb-1 text-gray-700">
          参会人(可选,逗号分隔)
        </label>
        <input
          type="text"
          value={participants}
          onChange={(e) => setParticipants(e.target.value)}
          disabled={disabled}
          className="w-full px-3 py-2 border border-gray-300 rounded focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:bg-gray-100"
          placeholder=""
        />
      </div>

      <div>
        <label className="block text-sm font-medium mb-1 text-gray-700">
          本次重点关注(可选,会议中也可以临时改)
        </label>
        <textarea
          value={focusPoints}
          onChange={(e) => setFocusPoints(e.target.value)}
          disabled={disabled}
          rows={2}
          className="w-full px-3 py-2 border border-gray-300 rounded focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:bg-gray-100 text-sm"
          placeholder={currentFocusPlaceholder}
        />
      </div>

      <button
        type="submit"
        disabled={!canSubmit}
        className="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 disabled:bg-gray-400 transition"
      >
        新建会议
      </button>
    </form>
  );
}
