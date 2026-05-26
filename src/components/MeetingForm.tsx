import { useState } from 'react';
import { MeetingDraft } from '../lib/tauri-bridge';

interface Props {
  onSubmit: (draft: MeetingDraft) => void;
  disabled?: boolean;
}

export function MeetingForm({ onSubmit, disabled }: Props) {
  const [name, setName] = useState('');
  const [projectRef, setProjectRef] = useState('');
  const [purpose, setPurpose] = useState('');
  const [participants, setParticipants] = useState('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    onSubmit({
      name: name.trim(),
      project_ref: projectRef.trim() || undefined,
      purpose: purpose.trim() || undefined,
      participants: participants.trim() || undefined,
    });
  };

  const canSubmit = !!name.trim() && !disabled;

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
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
          placeholder="例:陆家嘴连桥谈判"
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
          placeholder="例:陆家嘴连桥"
        />
      </div>

      <div>
        <label className="block text-sm font-medium mb-1 text-gray-700">
          会议目的(可选)
        </label>
        <select
          value={purpose}
          onChange={(e) => setPurpose(e.target.value)}
          disabled={disabled}
          className="w-full px-3 py-2 border border-gray-300 rounded focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:bg-gray-100"
        >
          <option value="">未指定</option>
          <option value="报价谈判">报价谈判</option>
          <option value="设计评审">设计评审</option>
          <option value="立项沟通">立项沟通</option>
          <option value="投标方案">投标方案</option>
          <option value="项目对接">项目对接</option>
          <option value="其他">其他</option>
        </select>
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
          placeholder="例:陆家嘴林总, 华东院李工"
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
