import React, { useState, useEffect } from 'react';
import { api } from '../services/api';
import { SystemStatus, AuthStatus } from '../types';
import { X, Key, Info, CheckCircle2 } from 'lucide-react';

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  onTokenUpdated: () => void;
}

export const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
  onClose,
  onTokenUpdated,
}) => {
  const [token, setToken] = useState(api.getAuthToken());
  const [provider, setProvider] = useState(() => localStorage.getItem('plexis_default_provider') || 'mock');
  const [openaiKey, setOpenaiKey] = useState(() => localStorage.getItem('plexis_openai_key') || '');
  const [geminiKey, setGeminiKey] = useState(() => localStorage.getItem('plexis_gemini_key') || '');
  const [anthropicKey, setAnthropicKey] = useState(() => localStorage.getItem('plexis_anthropic_key') || '');
  const [ollamaUrl, setOllamaUrl] = useState(() => localStorage.getItem('plexis_ollama_url') || 'http://localhost:11434');
  const [systemStatus, setSystemStatus] = useState<SystemStatus | null>(null);
  const [authStatus, setAuthStatus] = useState<AuthStatus | null>(null);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    if (isOpen) {
      setToken(api.getAuthToken());
      setProvider(localStorage.getItem('plexis_default_provider') || 'mock');
      setOpenaiKey(localStorage.getItem('plexis_openai_key') || '');
      setGeminiKey(localStorage.getItem('plexis_gemini_key') || '');
      setAnthropicKey(localStorage.getItem('plexis_anthropic_key') || '');
      setOllamaUrl(localStorage.getItem('plexis_ollama_url') || 'http://localhost:11434');
      api.getSystemStatus().then(setSystemStatus).catch(() => null);
      api.getAuthStatus().then(setAuthStatus).catch(() => null);
      setSaved(false);
    }
  }, [isOpen]);

  if (!isOpen) return null;

  const handleSave = (e: React.FormEvent) => {
    e.preventDefault();
    api.setAuthToken(token.trim());
    localStorage.setItem('plexis_default_provider', provider);
    localStorage.setItem('plexis_openai_key', openaiKey.trim());
    localStorage.setItem('plexis_gemini_key', geminiKey.trim());
    localStorage.setItem('plexis_anthropic_key', anthropicKey.trim());
    localStorage.setItem('plexis_ollama_url', ollamaUrl.trim());
    setSaved(true);
    onTokenUpdated();
    setTimeout(() => {
      onClose();
    }, 800);
  };

  return (
    <div className="fixed inset-0 bg-black/75 backdrop-blur-sm flex items-center justify-center z-50 p-4">
      <div className="bg-surface border border-surface-border rounded-xl max-w-lg w-full shadow-2xl overflow-hidden animate-in fade-in zoom-in-95 duration-150 max-h-[90vh] flex flex-col">
        <div className="px-6 py-4 border-b border-surface-border flex items-center justify-between bg-[#0e1424] shrink-0">
          <div className="flex items-center space-x-2">
            <Key className="w-4 h-4 text-indigo-400" />
            <h2 className="text-sm font-bold text-slate-100">Control Plane & Provider Configuration</h2>
          </div>
          <button
            onClick={onClose}
            className="p-1 text-slate-400 hover:text-slate-200 rounded hover:bg-surface-hover"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        <form onSubmit={handleSave} className="p-6 space-y-4 overflow-y-auto flex-1">
          <div>
            <label className="block text-xs font-semibold text-slate-300 uppercase tracking-wider mb-1.5">
              Bearer Authentication Token
            </label>
            <input
              type="password"
              placeholder="PLEXIS_AUTH_TOKEN value..."
              value={token}
              onChange={(e) => setToken(e.target.value)}
              className="w-full bg-[#0a0d14] border border-surface-border rounded-md px-3.5 py-2 text-xs text-slate-200 font-mono"
            />
            <p className="text-[11px] text-slate-400 mt-1">
              {authStatus?.auth_required
                ? 'Authentication is required by this Plexis daemon.'
                : 'Local loopback authentication is optional. Set token if daemon is protected.'}
            </p>
          </div>

          {/* Provider Selection */}
          <div className="pt-2 border-t border-surface-border space-y-3">
            <label className="block text-xs font-semibold text-slate-300 uppercase tracking-wider">
              Default LLM Provider
            </label>
            <select
              value={provider}
              onChange={(e) => setProvider(e.target.value)}
              className="w-full bg-[#0a0d14] border border-surface-border rounded-md px-3 py-2 text-xs text-slate-200 font-mono"
            >
              <option value="mock">Local Deterministic Mock (Testing / Offline)</option>
              <option value="openai">OpenAI (GPT-4o, o1, o3-mini)</option>
              <option value="gemini">Google Gemini (Gemini 1.5 Pro, 2.0 Flash)</option>
              <option value="anthropic">Anthropic Claude (Claude 3.5 Sonnet)</option>
              <option value="ollama">Ollama (Local Hermetic LLM)</option>
            </select>

            {provider === 'openai' && (
              <div>
                <label className="block text-[11px] text-slate-400 mb-1">OpenAI API Key (OPENAI_API_KEY)</label>
                <input
                  type="password"
                  placeholder="sk-proj-..."
                  value={openaiKey}
                  onChange={(e) => setOpenaiKey(e.target.value)}
                  className="w-full bg-[#0a0d14] border border-surface-border rounded-md px-3 py-1.5 text-xs text-slate-200 font-mono"
                />
              </div>
            )}

            {provider === 'gemini' && (
              <div>
                <label className="block text-[11px] text-slate-400 mb-1">Google Gemini API Key (GEMINI_API_KEY)</label>
                <input
                  type="password"
                  placeholder="AIzaSy..."
                  value={geminiKey}
                  onChange={(e) => setGeminiKey(e.target.value)}
                  className="w-full bg-[#0a0d14] border border-surface-border rounded-md px-3 py-1.5 text-xs text-slate-200 font-mono"
                />
              </div>
            )}

            {provider === 'anthropic' && (
              <div>
                <label className="block text-[11px] text-slate-400 mb-1">Anthropic API Key (ANTHROPIC_API_KEY)</label>
                <input
                  type="password"
                  placeholder="sk-ant-..."
                  value={anthropicKey}
                  onChange={(e) => setAnthropicKey(e.target.value)}
                  className="w-full bg-[#0a0d14] border border-surface-border rounded-md px-3 py-1.5 text-xs text-slate-200 font-mono"
                />
              </div>
            )}

            {provider === 'ollama' && (
              <div>
                <label className="block text-[11px] text-slate-400 mb-1">Ollama Base URL (OLLAMA_HOST)</label>
                <input
                  type="text"
                  placeholder="http://localhost:11434"
                  value={ollamaUrl}
                  onChange={(e) => setOllamaUrl(e.target.value)}
                  className="w-full bg-[#0a0d14] border border-surface-border rounded-md px-3 py-1.5 text-xs text-slate-200 font-mono"
                />
              </div>
            )}
          </div>

          {/* System Runtime Metrics */}
          {systemStatus && (
            <div className="p-3 bg-[#0a0d14] rounded-lg border border-surface-border space-y-2 text-xs font-mono">
              <div className="flex items-center space-x-1.5 text-indigo-400 font-semibold">
                <Info className="w-3.5 h-3.5" />
                <span>Runtime Node Status</span>
              </div>
              <div className="grid grid-cols-2 gap-2 text-slate-300">
                <div>Version: {systemStatus.version}</div>
                <div>Status: {systemStatus.status}</div>
                <div>Agents: {systemStatus.agents_count}</div>
                <div>Tools: {systemStatus.tools_count}</div>
                <div className="col-span-2">Uptime: {systemStatus.uptime_secs}s</div>
              </div>
            </div>
          )}

          {saved && (
            <div className="p-2.5 bg-emerald-950/40 border border-emerald-500/40 rounded text-xs text-emerald-300 flex items-center space-x-2">
              <CheckCircle2 className="w-4 h-4" />
              <span>Configuration saved successfully!</span>
            </div>
          )}

          <div className="flex items-center justify-end space-x-2 pt-3 border-t border-surface-border shrink-0">
            <button
              type="button"
              onClick={onClose}
              className="px-3.5 py-1.5 text-slate-400 hover:text-slate-200 text-xs"
            >
              Close
            </button>
            <button
              type="submit"
              className="px-4 py-1.5 bg-primary-600 hover:bg-primary-500 text-white rounded text-xs font-semibold"
            >
              Save Configuration
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
