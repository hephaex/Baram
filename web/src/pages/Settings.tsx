import { useState } from 'react';
import { Save, Play, Square, RefreshCw } from 'lucide-react';

interface CrawlerSettings {
  requests_per_second: number;
  max_concurrent_workers: number;
  timeout_secs: number;
  max_retries: number;
  comments_enabled: boolean;
}

interface LLMSettings {
  provider: 'ollama' | 'vllm';
  base_url: string;
  model: string;
  batch_size: number;
}

export function Settings() {
  const [crawlerSettings, setCrawlerSettings] = useState<CrawlerSettings>({
    requests_per_second: 5,
    max_concurrent_workers: 10,
    timeout_secs: 30,
    max_retries: 3,
    comments_enabled: true,
  });

  const [llmSettings, setLLMSettings] = useState<LLMSettings>({
    provider: 'vllm',
    base_url: 'http://localhost:8000',
    model: 'Qwen/Qwen3-8B',
    batch_size: 2,
  });

  const [crawlerStatus, setCrawlerStatus] = useState<'running' | 'stopped'>('stopped');
  const [isSaving, setIsSaving] = useState(false);

  const handleSave = async () => {
    setIsSaving(true);
    await new Promise((resolve) => setTimeout(resolve, 1000));
    setIsSaving(false);
    alert('설정이 저장되었습니다.');
  };

  const handleStartCrawler = () => {
    setCrawlerStatus('running');
  };

  const handleStopCrawler = () => {
    setCrawlerStatus('stopped');
  };

  return (
    <div className="p-6 max-w-4xl">
      {/* Header */}
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-gray-900">Settings</h1>
        <p className="text-gray-500">시스템 설정을 관리합니다</p>
      </div>

      {/* Crawler Control */}
      <section className="bg-white rounded-xl shadow-sm p-6 mb-6">
        <h2 className="text-lg font-semibold mb-4">크롤러 제어</h2>
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2">
            <div
              className={`w-3 h-3 rounded-full ${
                crawlerStatus === 'running' ? 'bg-green-500 animate-pulse' : 'bg-gray-400'
              }`}
            />
            <span className="font-medium">
              {crawlerStatus === 'running' ? '실행 중' : '중지됨'}
            </span>
          </div>
          <div className="flex gap-2">
            <button
              onClick={handleStartCrawler}
              disabled={crawlerStatus === 'running'}
              className="flex items-center gap-2 px-4 py-2 bg-green-600 text-white rounded-lg hover:bg-green-700 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <Play className="w-4 h-4" />
              시작
            </button>
            <button
              onClick={handleStopCrawler}
              disabled={crawlerStatus === 'stopped'}
              className="flex items-center gap-2 px-4 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <Square className="w-4 h-4" />
              중지
            </button>
          </div>
        </div>
      </section>

      {/* Crawler Settings */}
      <section className="bg-white rounded-xl shadow-sm p-6 mb-6">
        <h2 className="text-lg font-semibold mb-4">크롤러 설정</h2>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              초당 요청 수
            </label>
            <input
              type="number"
              value={crawlerSettings.requests_per_second}
              onChange={(e) =>
                setCrawlerSettings({
                  ...crawlerSettings,
                  requests_per_second: Number(e.target.value),
                })
              }
              min={1}
              max={20}
              className="w-full px-3 py-2 rounded-lg border border-gray-200 focus:border-blue-500 outline-none"
            />
            <p className="text-xs text-gray-500 mt-1">권장: 5 (네이버 정책 준수)</p>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              동시 워커 수
            </label>
            <input
              type="number"
              value={crawlerSettings.max_concurrent_workers}
              onChange={(e) =>
                setCrawlerSettings({
                  ...crawlerSettings,
                  max_concurrent_workers: Number(e.target.value),
                })
              }
              min={1}
              max={50}
              className="w-full px-3 py-2 rounded-lg border border-gray-200 focus:border-blue-500 outline-none"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              타임아웃 (초)
            </label>
            <input
              type="number"
              value={crawlerSettings.timeout_secs}
              onChange={(e) =>
                setCrawlerSettings({
                  ...crawlerSettings,
                  timeout_secs: Number(e.target.value),
                })
              }
              min={10}
              max={120}
              className="w-full px-3 py-2 rounded-lg border border-gray-200 focus:border-blue-500 outline-none"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              최대 재시도 횟수
            </label>
            <input
              type="number"
              value={crawlerSettings.max_retries}
              onChange={(e) =>
                setCrawlerSettings({
                  ...crawlerSettings,
                  max_retries: Number(e.target.value),
                })
              }
              min={0}
              max={10}
              className="w-full px-3 py-2 rounded-lg border border-gray-200 focus:border-blue-500 outline-none"
            />
          </div>

          <div className="md:col-span-2">
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={crawlerSettings.comments_enabled}
                onChange={(e) =>
                  setCrawlerSettings({
                    ...crawlerSettings,
                    comments_enabled: e.target.checked,
                  })
                }
                className="w-4 h-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
              />
              <span className="text-sm font-medium text-gray-700">댓글 수집 활성화</span>
            </label>
          </div>
        </div>
      </section>

      {/* LLM Settings */}
      <section className="bg-white rounded-xl shadow-sm p-6 mb-6">
        <h2 className="text-lg font-semibold mb-4">LLM 설정</h2>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Provider
            </label>
            <select
              value={llmSettings.provider}
              onChange={(e) =>
                setLLMSettings({
                  ...llmSettings,
                  provider: e.target.value as 'ollama' | 'vllm',
                })
              }
              className="w-full px-3 py-2 rounded-lg border border-gray-200 focus:border-blue-500 outline-none"
            >
              <option value="vllm">vLLM</option>
              <option value="ollama">Ollama</option>
            </select>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Base URL
            </label>
            <input
              type="text"
              value={llmSettings.base_url}
              onChange={(e) =>
                setLLMSettings({ ...llmSettings, base_url: e.target.value })
              }
              className="w-full px-3 py-2 rounded-lg border border-gray-200 focus:border-blue-500 outline-none"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Model
            </label>
            <input
              type="text"
              value={llmSettings.model}
              onChange={(e) =>
                setLLMSettings({ ...llmSettings, model: e.target.value })
              }
              className="w-full px-3 py-2 rounded-lg border border-gray-200 focus:border-blue-500 outline-none"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Batch Size
            </label>
            <input
              type="number"
              value={llmSettings.batch_size}
              onChange={(e) =>
                setLLMSettings({
                  ...llmSettings,
                  batch_size: Number(e.target.value),
                })
              }
              min={1}
              max={10}
              className="w-full px-3 py-2 rounded-lg border border-gray-200 focus:border-blue-500 outline-none"
            />
          </div>
        </div>
      </section>

      {/* Save Button */}
      <div className="flex justify-end">
        <button
          onClick={handleSave}
          disabled={isSaving}
          className="flex items-center gap-2 px-6 py-3 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50"
        >
          {isSaving ? (
            <RefreshCw className="w-4 h-4 animate-spin" />
          ) : (
            <Save className="w-4 h-4" />
          )}
          {isSaving ? '저장 중...' : '설정 저장'}
        </button>
      </div>
    </div>
  );
}
