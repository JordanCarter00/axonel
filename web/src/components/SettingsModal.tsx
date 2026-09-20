import React, { useState, useEffect } from 'react';
import { api } from '../services/api';
import { SystemStatus, AuthStatus } from '../types';
import { Info, CheckCircle2, Sliders } from 'lucide-react';
import { Modal, Button, Input, Select, StatusDot } from './ui';

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
  const [provider, setProvider] = useState(
    () => localStorage.getItem('axonel_default_provider') || localStorage.getItem('plexis_default_provider') || 'mock'
  );
  const [openaiKey, setOpenaiKey] = useState(
    () => localStorage.getItem('axonel_openai_key') || localStorage.getItem('plexis_openai_key') || ''
  );
  const [geminiKey, setGeminiKey] = useState(
    () => localStorage.getItem('axonel_gemini_key') || localStorage.getItem('plexis_gemini_key') || ''
  );
  const [anthropicKey, setAnthropicKey] = useState(
    () => localStorage.getItem('axonel_anthropic_key') || localStorage.getItem('plexis_anthropic_key') || ''
  );
  const [ollamaUrl, setOllamaUrl] = useState(
    () => localStorage.getItem('axonel_ollama_url') || localStorage.getItem('plexis_ollama_url') || 'http://localhost:11434'
  );
  const [systemStatus, setSystemStatus] = useState<SystemStatus | null>(null);
  const [authStatus, setAuthStatus] = useState<AuthStatus | null>(null);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    if (isOpen) {
      setToken(api.getAuthToken());
      setProvider(
        localStorage.getItem('axonel_default_provider') || localStorage.getItem('plexis_default_provider') || 'mock'
      );
      setOpenaiKey(
        localStorage.getItem('axonel_openai_key') || localStorage.getItem('plexis_openai_key') || ''
      );
      setGeminiKey(
        localStorage.getItem('axonel_gemini_key') || localStorage.getItem('plexis_gemini_key') || ''
      );
      setAnthropicKey(
        localStorage.getItem('axonel_anthropic_key') || localStorage.getItem('plexis_anthropic_key') || ''
      );
      setOllamaUrl(
        localStorage.getItem('axonel_ollama_url') || localStorage.getItem('plexis_ollama_url') || 'http://localhost:11434'
      );
      api.getSystemStatus().then(setSystemStatus).catch(() => null);
      api.getAuthStatus().then(setAuthStatus).catch(() => null);
      setSaved(false);
    }
  }, [isOpen]);

  if (!isOpen) return null;

  const handleSave = (e: React.FormEvent) => {
    e.preventDefault();
    api.setAuthToken(token.trim());
    localStorage.setItem('axonel_default_provider', provider);
    localStorage.setItem('plexis_default_provider', provider);
    localStorage.setItem('axonel_openai_key', openaiKey.trim());
    localStorage.setItem('plexis_openai_key', openaiKey.trim());
    localStorage.setItem('axonel_gemini_key', geminiKey.trim());
    localStorage.setItem('plexis_gemini_key', geminiKey.trim());
    localStorage.setItem('axonel_anthropic_key', anthropicKey.trim());
    localStorage.setItem('plexis_anthropic_key', anthropicKey.trim());
    localStorage.setItem('axonel_ollama_url', ollamaUrl.trim());
    localStorage.setItem('plexis_ollama_url', ollamaUrl.trim());
    setSaved(true);
    onTokenUpdated();
    setTimeout(() => {
      onClose();
    }, 800);
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title={
        <div className="flex items-center gap-2">
          <Sliders className="w-4 h-4 text-axonel-lime" />
          <span>Control Plane & Inference Configuration</span>
        </div>
      }
      subtitle="Daemon bearer token, LLM credentials, and runtime node health"
      maxWidth="lg"
      footer={
        <div className="w-full flex items-center justify-between">
          <div className="text-[11px] text-gray-500 font-mono">
            {saved ? (
              <span className="text-emerald-400 flex items-center gap-1.5 font-medium">
                <CheckCircle2 className="w-3.5 h-3.5" />
                Settings saved
              </span>
            ) : (
              'Credentials stored hermetically in local browser state'
            )}
          </div>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={onClose}
            >
              Close
            </Button>
            <Button
              type="button"
              variant="primary"
              size="sm"
              onClick={handleSave}
            >
              Save Configuration
            </Button>
          </div>
        </div>
      }
    >
      <form onSubmit={handleSave} className="space-y-4">
        <div>
          <Input
            label="Bearer Authentication Token"
            type="password"
            placeholder="AXONEL_AUTH_TOKEN value..."
            value={token}
            onChange={(e) => setToken(e.target.value)}
            mono
            helperText={
              authStatus?.auth_required
                ? 'Authentication is required by this Axonel daemon.'
                : 'Local loopback authentication is optional. Set token if daemon is protected.'
            }
          />
        </div>

        {/* Provider Selection */}
        <div className="pt-2 border-t border-surface-border space-y-3">
          <Select
            label="Default LLM Inference Provider"
            value={provider}
            onChange={(e) => setProvider(e.target.value)}
            mono
          >
            <option value="mock">Local Deterministic Mock (Testing / Offline)</option>
            <option value="openai">OpenAI (GPT-4o, o1, o3-mini)</option>
            <option value="gemini">Google Gemini (Gemini 1.5 Pro, 2.0 Flash)</option>
            <option value="anthropic">Anthropic Claude (Claude 3.5 Sonnet)</option>
            <option value="ollama">Ollama (Local Hermetic LLM)</option>
          </Select>

          {provider === 'openai' && (
            <Input
              label="OpenAI API Key (OPENAI_API_KEY)"
              type="password"
              placeholder="sk-proj-..."
              value={openaiKey}
              onChange={(e) => setOpenaiKey(e.target.value)}
              mono
            />
          )}

          {provider === 'gemini' && (
            <Input
              label="Google Gemini API Key (GEMINI_API_KEY)"
              type="password"
              placeholder="AIzaSy..."
              value={geminiKey}
              onChange={(e) => setGeminiKey(e.target.value)}
              mono
            />
          )}

          {provider === 'anthropic' && (
            <Input
              label="Anthropic API Key (ANTHROPIC_API_KEY)"
              type="password"
              placeholder="sk-ant-..."
              value={anthropicKey}
              onChange={(e) => setAnthropicKey(e.target.value)}
              mono
            />
          )}

          {provider === 'ollama' && (
            <Input
              label="Ollama Base URL (OLLAMA_HOST)"
              type="text"
              placeholder="http://localhost:11434"
              value={ollamaUrl}
              onChange={(e) => setOllamaUrl(e.target.value)}
              mono
            />
          )}
        </div>

        {/* System Runtime Metrics */}
        {systemStatus && (
          <div className="p-3 bg-surface-base rounded border border-surface-border space-y-2 text-xs font-mono">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-1.5 text-gray-300 font-semibold">
                <Info className="w-3.5 h-3.5 text-axonel-lime" />
                <span>Runtime Node Status</span>
              </div>
              <div className="flex items-center gap-1.5">
                <StatusDot status="active" />
                <span className="text-emerald-400 font-medium capitalize">{systemStatus.status}</span>
              </div>
            </div>
            <div className="grid grid-cols-2 gap-2 text-gray-400 pt-1 border-t border-surface-border">
              <div>Version: <span className="text-gray-200">{systemStatus.version}</span></div>
              <div>Status: <span className="text-gray-200">{systemStatus.status}</span></div>
              <div>Agents: <span className="text-gray-200">{systemStatus.agents_count}</span></div>
              <div>Tools: <span className="text-gray-200">{systemStatus.tools_count}</span></div>
              <div className="col-span-2">Uptime: <span className="text-gray-200">{systemStatus.uptime_secs}s</span></div>
            </div>
          </div>
        )}
      </form>
    </Modal>
  );
};

